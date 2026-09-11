"""Native A07 geometry and responsive-navigation qualification."""

import json
import math
import subprocess
import time


EDIT_MARKER = b"Needle begins in ordinary prose with caf\xc3\xa9 and \xe6\x9d\xb1\xe4\xba\xac."
MARKDOWN_ESCAPABLE_BYTES = frozenset(b'!"#$%&\'()*+,-./:;<=>?@[\\]^_`{|}~')


def _single(nodes, predicate, label):
    matches = [
        (index, node) for index, node in enumerate(nodes) if predicate(index, node)
    ]
    if len(matches) != 1:
        raise RuntimeError(f"Expected one {label}, found {len(matches)}")
    return matches[0]


def _descends_from(nodes, index, ancestor):
    seen = set()
    while index is not None and index not in seen:
        if index == ancestor:
            return True
        seen.add(index)
        index = nodes[index].get("parent")
    return False


def _numeric_bounds(node, label):
    bounds = node.get("bounds")
    if not isinstance(bounds, dict):
        raise RuntimeError(f"{label} has no native component bounds")
    values = tuple(bounds.get(key) for key in ("x", "y", "width", "height"))
    if not all(isinstance(value, (int, float)) and math.isfinite(value) for value in values):
        raise RuntimeError(f"{label} has invalid native bounds: {bounds}")
    x, y, width, height = values
    if width <= 0 or height <= 0:
        raise RuntimeError(f"{label} has non-positive native bounds: {bounds}")
    return dict(x=x, y=y, width=width, height=height)


def _latest_committed(reports):
    usable = [
        report for report in reports
        if report.get("committed") and report.get("scope") == "whole_document"
    ]
    if not usable:
        raise RuntimeError("No committed whole-document layout report is available")
    return usable, usable[-1]


def evaluate(nodes, reports, width, height, scale, navigation_visible):
    """Evaluate one stable AT-SPI snapshot against its published layout."""
    factor = scale / 120
    tolerance = 1.01
    output_width = round(width * factor)
    output_height = round(height * factor)

    editor_index, editor = _single(
        nodes,
        lambda _, node: node.get("role") == "entry"
        and node.get("name") == "Markdown document editor",
        "Markdown document editor",
    )
    document_index, document = _single(
        nodes,
        lambda index, node: node.get("role") == "document frame"
        and _descends_from(nodes, index, editor_index),
        "document frame beneath the editor",
    )
    _, close = _single(
        nodes,
        lambda _, node: node.get("role") == "button" and node.get("name") == "Close window",
        "Close window button",
    )
    editor_bounds = _numeric_bounds(editor, "editor")
    document_bounds = _numeric_bounds(document, "document")
    close_bounds = _numeric_bounds(close, "Close window button")

    expected_control = 28 * factor
    scale_error = max(
        abs(close_bounds["width"] - expected_control),
        abs(close_bounds["height"] - expected_control),
    )
    editor_inside_output = (
        editor_bounds["x"] >= -tolerance
        and editor_bounds["y"] >= -tolerance
        and editor_bounds["x"] + editor_bounds["width"] <= output_width + tolerance
        and editor_bounds["y"] + editor_bounds["height"] <= output_height + tolerance
    )
    document_inside_editor_horizontally = (
        document_bounds["x"] >= editor_bounds["x"] - tolerance
        and document_bounds["x"] + document_bounds["width"]
        <= editor_bounds["x"] + editor_bounds["width"] + tolerance
    )

    semantic_indexes = [
        index for index in range(len(nodes))
        if index != document_index and _descends_from(nodes, index, document_index)
    ]
    horizontal_violations = []
    for index in semantic_indexes:
        bounds = nodes[index].get("bounds")
        if not isinstance(bounds, dict) or bounds.get("width", 0) <= 0:
            continue
        left = bounds.get("x")
        right = left + bounds.get("width") if isinstance(left, (int, float)) else None
        if (
            right is None
            or left < document_bounds["x"] - tolerance
            or right > document_bounds["x"] + document_bounds["width"] + tolerance
        ):
            horizontal_violations.append(
                dict(index=index, role=nodes[index].get("role"), name=nodes[index].get("name"), bounds=bounds)
            )

    headings = [
        nodes[index].get("name")
        for index in semantic_indexes
        if nodes[index].get("role") == "heading"
    ]
    if not headings or any(not heading for heading in headings):
        raise RuntimeError(f"Document headings are missing or unnamed: {headings}")

    landmarks = [
        (index, node) for index, node in enumerate(nodes)
        if node.get("role") == "landmark" and node.get("name") == "Files and document outline"
    ]
    outline_names = []
    file_items = 0
    if navigation_visible:
        if len(landmarks) != 1:
            raise RuntimeError(f"Expected visible Files/Outline landmark, found {len(landmarks)}")
        landmark_index = landmarks[0][0]
        files_index, _ = _single(
            nodes,
            lambda index, node: node.get("role") == "tree"
            and node.get("name") == "Markdown folders and files"
            and _descends_from(nodes, index, landmark_index),
            "files tree",
        )
        outline_index, _ = _single(
            nodes,
            lambda index, node: node.get("role") == "tree"
            and node.get("name") == "Document outline"
            and _descends_from(nodes, index, landmark_index),
            "outline tree",
        )
        file_items = sum(
            node.get("role") == "tree item" and node.get("parent") == files_index
            for node in nodes
        )
        outline_names = [
            node.get("name") for node in nodes
            if node.get("role") == "tree item" and node.get("parent") == outline_index
        ]
        navigation_ok = file_items > 0 and outline_names == headings
    else:
        navigation_ok = not landmarks and not any(
            node.get("role") == "tree"
            and node.get("name") in ("Markdown folders and files", "Document outline")
            for node in nodes
        )

    usable, latest = _latest_committed(reports)
    layout_reports_ok = all(
        not report.get("planner_failed")
        and report.get("unresolved_image_dimensions") == 0
        and report.get("text_zoom") == 1.0
        for report in usable
    )
    canvas_error = abs(latest.get("canvas_width", -1) * factor - editor_bounds["width"])
    viewport_error = abs(latest.get("viewport_height", -1) * factor - editor_bounds["height"])
    anchor_displacements = [
        abs(report.get("anchor_displacement_at_commit_px", math.inf)) * factor
        for report in usable
    ]
    max_anchor_displacement = max(anchor_displacements, default=math.inf)
    text_interface_ok = (
        "Text" in editor.get("interfaces", [])
        and editor.get("character_count", -1) >= 0
        and editor.get("selection_count", -1) >= 0
    )

    checks = {
        "effective_fractional_scale": scale_error <= tolerance,
        "editor_inside_output": editor_inside_output,
        "document_inside_editor_horizontally": document_inside_editor_horizontally,
        "semantic_content_inside_document_horizontally": not horizontal_violations,
        "text_interface_readable": text_interface_ok,
        "responsive_navigation_state": navigation_ok,
        "outline_matches_document_headings": not navigation_visible or outline_names == headings,
        "committed_layout_resolved": layout_reports_ok,
        "canvas_matches_native_editor": canvas_error <= tolerance,
        "viewport_matches_native_editor": viewport_error <= tolerance,
        "anchor_within_one_physical_pixel": max_anchor_displacement <= 1.0 + 1e-6,
    }
    return {
        "passes": all(checks.values()),
        "checks": checks,
        "width": width,
        "height": height,
        "display_scale_120": scale,
        "physical_output": {"width": output_width, "height": output_height},
        "observed_control_size": close_bounds,
        "scale_error_px": scale_error,
        "editor_bounds": editor_bounds,
        "document_bounds": document_bounds,
        "canvas_width_logical": latest.get("canvas_width"),
        "viewport_height_logical": latest.get("viewport_height"),
        "canvas_error_px": canvas_error,
        "viewport_error_px": viewport_error,
        "committed_layout_reports": len(usable),
        "max_anchor_displacement_physical_px": max_anchor_displacement,
        "document_headings": headings,
        "outline_headings": outline_names,
        "file_items": file_items,
        "navigation_visible": navigation_visible,
        "horizontal_violations": horizontal_violations,
    }


def _probe_tree(env, pid, probe_path):
    result = subprocess.run(
        ["/usr/bin/python3", str(probe_path), str(pid)],
        env=env,
        capture_output=True,
        text=True,
        timeout=20,
    )
    if result.returncode:
        raise RuntimeError(result.stderr.strip() or "AT-SPI snapshot failed")
    return json.loads(result.stdout)["nodes"]


def _toggle_navigation(input_event):
    for key in (29, 56, 49):  # Ctrl+Alt+N
        input_event("key", key, 1)
    for key in (49, 56, 29):
        input_event("key", key, 0)


def exact_single_insertion_within(original, edited, marker, insertion=b"x"):
    """Return the exact insertion offset when it lies within one unique marker."""
    if (
        not marker
        or not insertion
        or original.count(marker) != 1
        or len(edited) != len(original) + len(insertion)
    ):
        return None
    marker_start = original.index(marker)
    marker_end = marker_start + len(marker)
    for offset in range(marker_start, marker_end + 1):
        if edited == original[:offset] + insertion + original[offset:]:
            return offset
    return None


def _unescape_markdown_punctuation(source):
    projected = bytearray()
    offset = 0
    while offset < len(source):
        if (
            source[offset] == ord("\\")
            and offset + 1 < len(source)
            and source[offset + 1] in MARKDOWN_ESCAPABLE_BYTES
        ):
            projected.append(source[offset + 1])
            offset += 2
        else:
            projected.append(source[offset])
            offset += 1
    return bytes(projected)


def semantic_single_insertion_within(original, edited, marker, insertion=b"x"):
    """Accept one target insertion plus serializer-added punctuation escapes."""
    return exact_single_insertion_within(
        original, _unescape_markdown_punctuation(edited), marker, insertion
    )


def _wait_for_source(source_path, predicate, label):
    deadline = time.monotonic() + 5
    value = source_path.read_bytes()
    while not predicate(value) and time.monotonic() < deadline:
        time.sleep(0.05)
        value = source_path.read_bytes()
    if not predicate(value):
        raise RuntimeError(f"Timed out waiting for {label}")
    return value


def _edit_and_restore(nodes, input_event, source_path, original_source, scale):
    marker_text = EDIT_MARKER.decode()
    _, paragraph = _single(
        nodes,
        lambda _, node: node.get("role") == "paragraph"
        and node.get("name") == marker_text,
        "ordinary-prose native edit target",
    )
    bounds = _numeric_bounds(paragraph, "ordinary-prose native edit target")
    factor = scale / 120
    point = {
        "x": round((bounds["x"] + bounds["width"] / 2) / factor),
        "y": round((bounds["y"] + bounds["height"] / 2) / factor),
    }
    input_event("move", point["x"], point["y"])
    input_event("button", 272, 1)
    input_event("button", 272, 0)
    input_event("key", 45, 1)  # X
    input_event("key", 45, 0)

    edited = _wait_for_source(
        source_path, lambda value: value != original_source, "native edit autosave"
    )
    insertion_offset = semantic_single_insertion_within(
        original_source, edited, EDIT_MARKER
    )
    if insertion_offset is None:
        changed = next(
            (index for index, pair in enumerate(zip(original_source, edited)) if pair[0] != pair[1]),
            min(len(original_source), len(edited)),
        )
        raise RuntimeError(
            "Native edit was not one semantic x inside the target paragraph: "
            f"first_change={changed}, "
            f"before={original_source[max(0, changed - 40):changed + 120]!r}, "
            f"after={edited[max(0, changed - 40):changed + 120]!r}"
        )

    input_event("key", 29, 1)  # Ctrl+Z
    input_event("key", 44, 1)
    input_event("key", 44, 0)
    input_event("key", 29, 0)
    _wait_for_source(
        source_path, lambda value: value == original_source, "byte-exact native undo"
    )
    return {
        "target": marker_text,
        "logical_input_point": point,
        "single_x_insertion_within_target": True,
        "only_lossless_markdown_escaping_besides_insertion": True,
        "insertion_byte_offset": insertion_offset,
        "serializer_escape_bytes": len(edited) - len(original_source) - 1,
        "autosaved_bytes": len(edited),
        "undo_restores_exact_bytes": True,
    }


def _wait_for_state(env, pid, probe_path, reports, width, height, scale, visible):
    deadline = time.monotonic() + 12
    last_error = None
    while time.monotonic() < deadline:
        try:
            nodes = _probe_tree(env, pid, probe_path)
            result = evaluate(nodes, reports(), width, height, scale, visible)
            if result["passes"]:
                return result
            last_error = result
        except (RuntimeError, subprocess.TimeoutExpired, json.JSONDecodeError) as error:
            last_error = str(error)
        time.sleep(0.1)
    raise RuntimeError(f"A07 native state did not stabilize: {last_error}")


def check(initial_nodes, env, input_event, source_path, original_source, pid, probe_path,
          reports, width, height, scale):
    """Exercise navigation and one exact native edit in an isolated app."""
    wide = width >= 800
    baseline = evaluate(initial_nodes, reports(), width, height, scale, wide)
    if not baseline["passes"]:
        raise RuntimeError(f"Initial A07 geometry failed: {baseline}")

    toggled = False
    try:
        _toggle_navigation(input_event)
        toggled = True
        alternate = _wait_for_state(
            env, pid, probe_path, reports, width, height, scale, not wide
        )
        _toggle_navigation(input_event)
        toggled = False
        restored = _wait_for_state(env, pid, probe_path, reports, width, height, scale, wide)
        edit = _edit_and_restore(
            _probe_tree(env, pid, probe_path), input_event, source_path,
            original_source, scale,
        )
        after_undo = _wait_for_state(
            env, pid, probe_path, reports, width, height, scale, wide
        )
    finally:
        if toggled:
            _toggle_navigation(input_event)

    source_unchanged = source_path.read_bytes() == original_source
    passes = (
        baseline["passes"]
        and alternate["passes"]
        and restored["passes"]
        and edit["single_x_insertion_within_target"]
        and edit["undo_restores_exact_bytes"]
        and after_undo["passes"]
        and source_unchanged
    )
    return {
        "passes": passes,
        "responsive_breakpoint": 800,
        "default_navigation_visible": wide,
        "alternate_navigation_visible": not wide,
        "navigation_restored": restored["navigation_visible"] == wide,
        "native_edit": edit,
        "geometry_after_undo": after_undo,
        "source_unchanged": source_unchanged,
        "baseline": baseline,
        "alternate": alternate,
        "restored": restored,
    }
