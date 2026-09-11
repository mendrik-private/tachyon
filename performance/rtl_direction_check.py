"""Native oracle for RTL hit testing and visual horizontal caret movement."""

import hashlib
import json
import subprocess
import time


RTL_TEXT = (
    "مرحبا بالعالم — يحافظ التخطيط التلقائي على ترتيب المصدر وموضع القراءة عند تغيير الخط."
)
SECTION = "3. Mixed direction and scoped overflow"


def evaluate(right, left, center, moved_left, moved_right, line_bounds,
             original_source, final_source):
    def caret_x(state):
        bounds = state.get("caret_bounds")
        return None if bounds is None else bounds[0]

    right_x = caret_x(right)
    left_x = caret_x(left)
    center_x = caret_x(center)
    moved_left_x = caret_x(moved_left)
    moved_right_x = caret_x(moved_right)
    collapsed = all(
        state["selection_start"] == state["selection_end"]
        for state in (right, left, center, moved_left, moved_right)
    )
    checks = {
        "rtl_hit_test_reverses_logical_offsets": (
            right["selection_start"] < left["selection_start"]
        ),
        "hit_test_covers_visible_line": (
            right_x is not None and left_x is not None
            and right_x > left_x + line_bounds["width"] * 0.5
        ),
        "left_arrow_moves_visually_left": (
            center_x is not None and moved_left_x is not None
            and moved_left_x < center_x - 0.5
        ),
        "right_arrow_moves_visually_right": (
            center_x is not None and moved_right_x is not None
            and moved_right_x > center_x + 0.5
        ),
        "carets_remain_collapsed": collapsed,
        "source_unchanged": final_source == original_source,
    }
    return {
        "line_bounds": line_bounds,
        "states": {
            "right_hit": right,
            "left_hit": left,
            "center_hit": center,
            "left_arrow": moved_left,
            "right_arrow": moved_right,
        },
        "checks": checks,
        "passes": all(checks.values()),
    }


def check(env, input_event, source_path, pid, probe_path, log_path):
    original = source_path.read_bytes()

    def probe(scroll_to=None):
        command = ["/usr/bin/python3", str(probe_path), str(pid)]
        if scroll_to is not None:
            command.append(f"--scroll-to-name={scroll_to}")
        result = subprocess.run(
            command, env=env, capture_output=True, text=True, timeout=30
        )
        if result.returncode:
            raise RuntimeError(result.stderr)
        return json.loads(result.stdout)

    def state():
        prefix = "MINERAL_RESIZE_STATE "
        before = sum(
            line.startswith(prefix) for line in log_path.read_text().splitlines()
        )
        input_event("key", 66, 1)  # F8
        input_event("key", 66, 0)
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            reports = [
                json.loads(line[len(prefix):])
                for line in log_path.read_text().splitlines()
                if line.startswith(prefix)
            ]
            if len(reports) > before:
                return reports[-1]
            time.sleep(0.02)
        raise RuntimeError("Validation binary did not report native caret geometry")

    def click(x, y):
        input_event("move", round(x), round(y))
        input_event("button", 272, 1)
        input_event("button", 272, 0)
        time.sleep(0.08)
        report = state()
        # Keep independent hit samples outside the compositor/editor's native
        # multi-click interval; otherwise the centre sample becomes a word or
        # paragraph selection rather than a caret placement.
        time.sleep(0.55)
        return report

    def settled_geometry():
        geometry = None
        previous = None
        stable = 0
        deadline = time.monotonic() + 6
        while time.monotonic() < deadline:
            candidate = state()
            signature = (candidate["scroll_y"], candidate.get("rtl_line_bounds", []))
            if signature == previous:
                stable += 1
            else:
                stable = 0
                previous = signature
            geometry = candidate
            if stable >= 2:
                break
            time.sleep(0.12)
        if stable < 2:
            raise RuntimeError("RTL line geometry did not settle after accessible reveal")
        return geometry

    def choose_line(geometry):
        snapshot = probe()
        matches = [node for node in snapshot["nodes"] if node["name"] == RTL_TEXT]
        if len(matches) != 1:
            raise RuntimeError(f"Expected one RTL text node, found {len(matches)}")
        accessible = matches[0]["screen_bounds"]
        candidates = [
            bounds for bounds in geometry.get("rtl_line_bounds", [])
            if accessible["y"] <= (bounds[1] + bounds[3]) * 0.5
            <= accessible["y"] + accessible["height"]
        ]
        if not candidates:
            raise RuntimeError("Validation binary reported no painted RTL line")
        return max(candidates, key=lambda bounds: bounds[2] - bounds[0]), accessible

    probe(SECTION)
    initial, _ = choose_line(settled_geometry())
    # Focusing a new canonical group can trigger one automatic layout-lock
    # replan. Complete that transition before taking independent hit samples.
    click((initial[0] + initial[2]) * 0.5, (initial[1] + initial[3]) * 0.5)
    probe(SECTION)
    (left_edge, top, right_edge, bottom), accessible_line = choose_line(
        settled_geometry()
    )
    line = {
        "x": left_edge,
        "y": top,
        "width": right_edge - left_edge,
        "height": bottom - top,
        "accessible_paragraph": accessible_line,
    }
    y = top + (bottom - top) * 0.5
    # The document scrollbar intentionally lives at the outer window edge.
    # Keep the trailing sample inside both the shaped line and the accessible
    # paragraph viewport so this remains a text hit, even when a glyph overhang
    # extends a few pixels into the reserved trailing inset.
    safe_right = min(
        right_edge - 4,
        accessible_line["x"] + accessible_line["width"] - 4,
    )
    safe_left = left_edge + 4
    left = click(safe_left, y)
    right = click(safe_right, y)
    center_x = (safe_left + safe_right) * 0.5
    center = click(center_x, y)

    input_event("key", 105, 1)  # Left
    input_event("key", 105, 0)
    moved_left = state()

    center = click(center_x, y)
    input_event("key", 106, 1)  # Right
    input_event("key", 106, 0)
    moved_right = state()

    result = evaluate(
        right, left, center, moved_left, moved_right, line,
        original, source_path.read_bytes(),
    )
    result.update(
        fixture=source_path.name,
        original_sha256=hashlib.sha256(original).hexdigest(),
    )
    return result
