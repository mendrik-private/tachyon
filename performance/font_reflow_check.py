"""Native oracle for body-font remeasurement and stable automatic reflow."""

import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import time


ANCHOR = "2. Reading anchor"
HEADING_NAMES = (
    "Typography, direction, and bounded overflow",
    "1. Measured body copy",
    ANCHOR,
    "Stable identities",
    "3. Mixed direction and scoped overflow",
    "4. Closing section",
)


def _one(nodes, role, name):
    matches = [node for node in nodes if node["role"] == role and node["name"] == name]
    if len(matches) != 1:
        raise RuntimeError(f"Expected one {role} named {name!r}, found {len(matches)}")
    return matches[0]


def _heading_identities(nodes):
    return [(node["path"], node["name"]) for node in nodes if node["role"] == "heading"]


def _anchor_offset(nodes):
    viewport = _one(nodes, "entry", "Markdown document editor")["bounds"]
    anchor = _one(nodes, "heading", ANCHOR)["bounds"]
    return anchor["y"] - viewport["y"]


def _heading_offsets(nodes):
    anchor_y = _one(nodes, "heading", ANCHOR)["bounds"]["y"]
    return {
        name: _one(nodes, "heading", name)["bounds"]["y"] - anchor_y
        for name in HEADING_NAMES
    }


def _visible_heading_bounds(nodes):
    viewport = _one(nodes, "entry", "Markdown document editor")["bounds"]
    top = viewport["y"]
    bottom = top + viewport["height"]
    return [
        (node["name"], node["bounds"])
        for node in nodes
        if node["role"] == "heading"
        and node["bounds"]["y"] + node["bounds"]["height"] >= top
        and node["bounds"]["y"] <= bottom
    ]


def _measurement_count(report, key):
    return sum(stage.get("measurements", {}).get(key, 0) for stage in report.get("stages", []))


def evaluate(before, alternate, repeated, restored, alternate_report, restored_report,
             original_source, final_source):
    snapshots = (before, alternate, repeated, restored)
    nodes = [snapshot["nodes"] for snapshot in snapshots]
    identities = [_heading_identities(item) for item in nodes]
    anchor_offsets = [_anchor_offset(item) for item in nodes]
    offsets = [_heading_offsets(item) for item in nodes]
    changed_offsets = {
        name: offsets[1][name] - offsets[0][name]
        for name in HEADING_NAMES
    }
    restored_deltas = {
        name: offsets[3][name] - offsets[0][name]
        for name in HEADING_NAMES
    }
    visible_geometry_changed = any(abs(delta) >= 2 for delta in changed_offsets.values())
    alternate_shaping = _measurement_count(alternate_report, "shaping_calls")
    restored_shaping = _measurement_count(restored_report, "shaping_calls")
    alternate_wrap_misses = _measurement_count(alternate_report, "wrap_cache_misses")
    restored_wrap_misses = _measurement_count(restored_report, "wrap_cache_misses")
    checks = {
        "heading_identities_stable": all(identity == identities[0] for identity in identities[1:]),
        "reading_anchor_stable": max(anchor_offsets) - min(anchor_offsets) <= 4,
        "font_change_measured_reflow": (
            alternate_report.get("committed") is True
            and alternate_shaping > 0
            and alternate_wrap_misses > 0
        ),
        "same_font_is_stable": _visible_heading_bounds(nodes[1]) == _visible_heading_bounds(nodes[2]),
        "restored_geometry": all(abs(delta) <= 2 for delta in restored_deltas.values()),
        "alternate_plan_committed": alternate_report.get("committed") is True,
        "alternate_font_reshaped": alternate_shaping > 0,
        "restored_plan_committed": restored_report.get("committed") is True,
        "restored_font_reshaped": restored_shaping > 0,
        "source_unchanged": final_source == original_source,
    }
    return {
        "anchor_offsets": anchor_offsets,
        "heading_offsets": offsets,
        "changed_offsets": changed_offsets,
        "visible_geometry_changed": visible_geometry_changed,
        "restored_deltas": restored_deltas,
        "alternate_trace": {
            "sequence": alternate_report.get("sequence"),
            "geometry_generation": alternate_report.get("geometry_generation"),
            "shaping_calls": alternate_shaping,
            "wrap_cache_misses": alternate_wrap_misses,
        },
        "restored_trace": {
            "sequence": restored_report.get("sequence"),
            "geometry_generation": restored_report.get("geometry_generation"),
            "shaping_calls": restored_shaping,
            "wrap_cache_misses": restored_wrap_misses,
        },
        "checks": checks,
        "passes": all(checks.values()),
    }


def _capture(env, work, output, stage):
    directory = work / f"font-{stage}"
    directory.mkdir()
    subprocess.run(["weston-screenshooter"], cwd=directory, env=env, check=True, timeout=10)
    screenshot, = directory.glob("*.png")
    shutil.copyfile(screenshot, output.with_name(f"{output.stem}-{stage}.png"))


def check(env, input_event, source_path, pid, output, probe_path, work, log_path):
    original = source_path.read_bytes()

    def probe(scroll_to=None):
        command = ["/usr/bin/python3", str(probe_path), str(pid)]
        if scroll_to is not None:
            command.append(f"--scroll-to-name={scroll_to}")
        result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=30)
        if result.returncode:
            raise RuntimeError(result.stderr)
        return json.loads(result.stdout)

    def reports():
        prefix = "MINERAL_LAYOUT_TRACE "
        return [
            json.loads(line[len(prefix):])
            for line in log_path.read_text().splitlines()
            if line.startswith(prefix)
        ]

    def switch_font(key, marker, loaded=None):
        marker_prefix = f"MINERAL_FONT_VALIDATION font={marker}"
        marker_count = sum(line.startswith(marker_prefix)
                           for line in log_path.read_text().splitlines())
        before_reports = len(reports())
        input_event("key", key, 1)
        input_event("key", key, 0)
        deadline = time.monotonic() + 20
        while True:
            lines = log_path.read_text().splitlines()
            matching_markers = [line for line in lines if line.startswith(marker_prefix)]
            marker_seen = len(matching_markers) > marker_count
            load_state_seen = loaded is None or any(
                line == f"{marker_prefix} loaded={str(loaded).lower()}"
                for line in matching_markers[marker_count:]
            )
            candidates = [report for report in reports()[before_reports:]
                          if report.get("committed") is True
                          and _measurement_count(report, "shaping_calls") > 0]
            if marker_seen and load_state_seen and candidates:
                time.sleep(0.8)
                return candidates[-1]
            if time.monotonic() >= deadline:
                raise RuntimeError(
                    f"Body-font switch to {marker!r} did not produce a committed measured plan"
                )
            time.sleep(0.04)

    # AccessKit currently maps AT-SPI's directional scroll request to the
    # generic ScrollIntoView action. Move to a known offscreen sibling first,
    # so revealing the anchor cannot legally no-op merely because it happened
    # to be visible near the bottom of the initial viewport.
    probe("4. Closing section")
    probe(ANCHOR)
    time.sleep(0.3)
    before = probe()
    _capture(env, work, output, "before")

    alternate_report = switch_font(64, "Noto Sans Mineral", loaded=True)  # F6
    alternate = probe()
    _capture(env, work, output, "alternate")

    input_event("key", 64, 1)  # Applying the same environment must be idempotent.
    input_event("key", 64, 0)
    time.sleep(1.0)
    repeated = probe()

    restored_report = switch_font(65, "Spline Sans Mineral")  # F7
    restored = probe()
    _capture(env, work, output, "restored")

    result = evaluate(
        before, alternate, repeated, restored, alternate_report, restored_report,
        original, source_path.read_bytes(),
    )
    result.update(
        fixture=source_path.name,
        original_sha256=hashlib.sha256(original).hexdigest(),
    )
    return result
