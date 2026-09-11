"""Native AT-16/17 oracle for the automatic-only layout boundary.

The validation build exposes small window-resize commands, never layout
choices.  This check walks a real measured peer row to its first stacked
fallback, then alternates the two adjacent widths.  The previous legal stack
must win hysteresis while the narrower width must never retain invalid columns.
"""

import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import time


PEERS = ("Document core", "Document view", "Application")
EDITOR = ("entry", "Markdown document editor")
LAYOUT_CONTROL_WORDS = frozenset(("plain", "adaptive", "focus", "layout"))


def _one(nodes, role, name):
    matches = [node for node in nodes if node["role"] == role and node["name"] == name]
    if len(matches) != 1:
        raise RuntimeError(f"Expected one {role} named {name!r}, found {len(matches)}")
    return matches[0]


def peer_layout(nodes):
    bounds = [_one(nodes, "heading", name)["bounds"] for name in PEERS]
    row = (max(bound["y"] for bound in bounds) - min(bound["y"] for bound in bounds) <= 2
           and all(left["x"] + left["width"] <= right["x"]
                   for left, right in zip(bounds, bounds[1:])))
    stack = (all(upper["y"] + upper["height"] < lower["y"]
                 for upper, lower in zip(bounds, bounds[1:]))
             and max(bound["x"] for bound in bounds) - min(bound["x"] for bound in bounds) <= 2)
    return "row" if row else "stack" if stack else "invalid"


def evaluate(samples, restored, original_source, final_source):
    layouts = [sample["layout"] for sample in samples]
    transition = next((index for index, layout in enumerate(layouts) if layout == "stack"), None)
    before_transition = samples[transition - 1] if transition not in (None, 0) else None
    after_transition = samples[transition] if transition is not None else None
    oscillation = samples[transition + 1:] if transition is not None else []
    initial_paths = samples[0]["heading_paths"] if samples else []
    forbidden_controls = [control for sample in samples + [restored]
                          for control in sample["layout_controls"]]
    checks = {
        "starts_as_measured_row": bool(samples) and samples[0]["layout"] == "row",
        "adjacent_width_fallback_found": (
            before_transition is not None and after_transition is not None
            and before_transition["width"] - after_transition["width"] <= 10
            and before_transition["layout"] == "row"
            and after_transition["layout"] == "stack"
        ),
        "no_invalid_intermediate_geometry": all(layout in ("row", "stack") for layout in layouts),
        "hysteresis_prevents_adjacent_thrashing": (
            len(oscillation) >= 6 and all(sample["layout"] == "stack" for sample in oscillation)
        ),
        "stable_semantic_identities": all(
            sample["heading_paths"] == initial_paths for sample in samples + [restored]
        ),
        "automatic_only_no_layout_controls": not forbidden_controls,
        "expanded_layout_remains_legal": restored["layout"] in ("row", "stack"),
        "layout_did_not_change_source_or_undo": final_source == original_source,
    }
    return {
        "samples": samples,
        "transition": {
            "last_row_width": before_transition["width"] if before_transition else None,
            "first_stack_width": after_transition["width"] if after_transition else None,
        },
        "restored": restored,
        "forbidden_controls": forbidden_controls,
        "original_sha256": hashlib.sha256(original_source).hexdigest(),
        "final_sha256": hashlib.sha256(final_source).hexdigest(),
        "checks": checks,
        "passes": all(checks.values()),
    }


def _capture(env, work, output, stage):
    directory = work / f"boundary-{stage}"
    directory.mkdir()
    subprocess.run(["weston-screenshooter"], cwd=directory, env=env, check=True, timeout=10)
    screenshot, = directory.glob("*.png")
    shutil.copyfile(screenshot, output.with_name(f"{output.stem}-{stage}.png"))


def check(env, input_event, source_path, pid, output, probe_path, work):
    original = source_path.read_bytes()

    def probe(scroll_to=None):
        command = ["/usr/bin/python3", str(probe_path), str(pid)]
        if scroll_to is not None:
            command.append(f"--scroll-to-name={scroll_to}")
        result = subprocess.run(
            command,
            env=env, capture_output=True, text=True, timeout=30,
        )
        if result.returncode:
            raise RuntimeError(result.stderr)
        nodes = json.loads(result.stdout)["nodes"]
        controls = []
        for node in nodes:
            if node["role"] not in ("push button", "toggle button", "radio button", "menu item"):
                continue
            words = set(node["name"].lower().replace(":", " ").split())
            if words & LAYOUT_CONTROL_WORDS:
                controls.append({"role": node["role"], "name": node["name"], "path": node["path"]})
        return {
            "width": _one(nodes, *EDITOR)["bounds"]["width"],
            "layout": peer_layout(nodes),
            "heading_paths": [(node["path"], node["name"])
                              for node in nodes if node["role"] == "heading"],
            "layout_controls": controls,
        }

    def press(key):
        input_event("key", key, 1)
        input_event("key", key, 0)

    def await_changed(previous_width, direction):
        deadline = time.monotonic() + 5
        while True:
            sample = probe()
            delta = sample["width"] - previous_width
            if (direction < 0 and delta <= -4) or (direction > 0 and delta >= 4):
                time.sleep(0.12)
                return probe()
            if time.monotonic() >= deadline:
                raise RuntimeError(
                    f"Validation resize did not move {direction:+} from {previous_width}: {sample['width']}"
                )
            time.sleep(0.03)

    initial = probe()
    press(68)  # F10: unambiguously wide.
    deadline = time.monotonic() + 5
    wide = probe()
    while abs(wide["width"] - initial["width"]) < 4 and time.monotonic() < deadline:
        time.sleep(0.03)
        wide = probe()
    probe("3. Architecture and persistence")
    time.sleep(1.)
    time.sleep(0.12)
    wide = probe()
    samples = [wide]
    _capture(env, work, output, "wide")

    for _ in range(100):
        press(87)  # F11: 8 logical px narrower.
        sample = await_changed(samples[-1]["width"], -1)
        samples.append(sample)
        if sample["layout"] == "stack":
            break
    else:
        raise RuntimeError("No measured row-to-stack boundary found in 100 small resizes")

    _capture(env, work, output, "first-stack")
    # Alternate exactly the two widths around the first transition.  Once the
    # legal stack wins, the 10% policy should retain it at the adjacent wider
    # width instead of changing topology on every pointer-sized resize.
    for key, direction in ((88, 1), (87, -1)) * 4:  # F12 / F11
        press(key)
        samples.append(await_changed(samples[-1]["width"], direction))

    # A layout-only sequence must not have created a content undo entry.
    input_event("key", 29, 1)  # Ctrl
    input_event("key", 44, 1)  # Z
    input_event("key", 44, 0)
    input_event("key", 29, 0)
    time.sleep(0.25)

    before_restore = samples[-1]["width"]
    press(68)  # F10
    restored = await_changed(before_restore, 1)
    time.sleep(1.)
    restored = probe()
    _capture(env, work, output, "restored-wide")
    return evaluate(samples, restored, original, source_path.read_bytes())
