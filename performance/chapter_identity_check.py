"""Native AT-SPI and editing oracle for chapter barriers and duplicate identities."""

import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import time


TARGET = "beta edit target"
TARGET_PARAGRAPH = (
    "The shared sentence remains deliberately identical. beta edit target belongs only to "
    "the second chapter."
)
MARKDOWN_ESCAPABLE_BYTES = frozenset(b'!"#$%&\'()*+,-./:;<=>?@[\\]^_`{|}~')


def _content_nodes(nodes):
    return [(index, node) for index, node in enumerate(nodes)
            if node["role"] in {
                "heading", "paragraph", "static", "list", "list item", "check box",
                "table", "table row", "table cell", "column header", "row header",
                "image", "math", "blockquote",
            }]


def _unescape_markdown_punctuation(source):
    projected = bytearray()
    offset = 0
    while offset < len(source):
        if (source[offset] == ord("\\") and offset + 1 < len(source)
                and source[offset + 1] in MARKDOWN_ESCAPABLE_BYTES):
            projected.append(source[offset + 1])
            offset += 2
        else:
            projected.append(source[offset])
            offset += 1
    return bytes(projected)


def _duplicate_records(nodes, role, name, count):
    records = [(index, node) for index, node in enumerate(nodes)
               if node["role"] == role and node["name"] == name]
    if len(records) != count:
        raise RuntimeError(f"Expected {count} {role} nodes named {name!r}, found {len(records)}")
    if len({node["path"] for _, node in records}) != count:
        raise RuntimeError(f"Duplicate {name!r} nodes share an accessibility identity")
    return records


def evaluate_snapshot(nodes):
    chapters = _duplicate_records(nodes, "heading", "Repeated chapter", 2)
    repeated = _duplicate_records(nodes, "heading", "Repeated heading", 2)
    details = _duplicate_records(nodes, "heading", "Repeated details", 2)
    target = next(((index, node) for index, node in enumerate(nodes)
                   if node["role"] == "paragraph" and TARGET in node["name"]), None)
    if target is None:
        raise RuntimeError("The second duplicate paragraph is missing")
    first_start, second_start = chapters[0][0], chapters[1][0]
    if not first_start < repeated[0][0] < details[0][0] < second_start:
        raise RuntimeError("First chapter hierarchy no longer follows authored order")
    if not second_start < repeated[1][0] < target[0] < details[1][0]:
        raise RuntimeError("Second chapter hierarchy no longer follows authored order")

    first_chapter = [node for index, node in _content_nodes(nodes)
                     if first_start <= index < second_start]
    first_bottom = max(node["bounds"]["y"] + node["bounds"]["height"]
                       for node in first_chapter)
    second_top = chapters[1][1]["bounds"]["y"]
    chapter_barrier = first_bottom <= second_top + 1
    if not chapter_barrier:
        raise RuntimeError(
            f"Second chapter interleaves the first: first bottom={first_bottom}, second top={second_top}"
        )
    heading_order = [node["name"] for _, node in _content_nodes(nodes)
                     if node["role"] == "heading"]
    expected = [
        "Chapter and duplicate identity", "Repeated chapter", "Repeated heading",
        "Repeated details", "Repeated chapter", "Repeated heading", "Repeated details",
    ]
    if heading_order != expected:
        raise RuntimeError(f"Heading traversal changed: {heading_order!r}")
    return {
        "heading_order": heading_order,
        "chapter_paths": [node["path"] for _, node in chapters],
        "repeated_heading_paths": [node["path"] for _, node in repeated],
        "repeated_detail_paths": [node["path"] for _, node in details],
        "target_path": target[1]["path"],
        "target_name": target[1]["name"],
        "first_chapter_bottom": first_bottom,
        "second_chapter_top": second_top,
        "chapter_barrier": chapter_barrier,
    }


def evaluate_transition(before, edited, restored, original_source, edited_source, restored_source):
    before_eval = evaluate_snapshot(before)
    edited_eval = evaluate_snapshot(edited)
    restored_eval = evaluate_snapshot(restored)
    identities = (
        before_eval["chapter_paths"] + before_eval["repeated_heading_paths"]
        + before_eval["repeated_detail_paths"]
    )
    edited_identities = (
        edited_eval["chapter_paths"] + edited_eval["repeated_heading_paths"]
        + edited_eval["repeated_detail_paths"]
    )
    restored_identities = (
        restored_eval["chapter_paths"] + restored_eval["repeated_heading_paths"]
        + restored_eval["repeated_detail_paths"]
    )
    chapter_marker = b"\n## Repeated chapter\n"
    first_chapter_offset = original_source.index(chapter_marker)
    second_chapter_offset = original_source.index(
        chapter_marker, first_chapter_offset + len(chapter_marker)
    )
    semantic_edited = _unescape_markdown_punctuation(edited_source)
    checks = {
        "duplicate_heading_identities_distinct": len(set(identities)) == len(identities),
        "heading_identities_stable": identities == edited_identities == restored_identities,
        "target_identity_stable": (
            before_eval["target_path"] == edited_eval["target_path"] == restored_eval["target_path"]
        ),
        "target_edit_exact": (
            before_eval["target_name"] == TARGET_PARAGRAPH
            and edited_eval["target_name"] == TARGET_PARAGRAPH.replace(TARGET, TARGET + "x")
            and restored_eval["target_name"] == TARGET_PARAGRAPH
            and (TARGET + "x").encode() in semantic_edited
        ),
        "first_chapter_bytes_unchanged": (
            edited_source[:second_chapter_offset] == original_source[:second_chapter_offset]
        ),
        "edited_second_chapter_only": edited_source != original_source,
        "undo_source_exact": restored_source == original_source,
        "chapter_barrier_before": before_eval["chapter_barrier"],
        "chapter_barrier_edited": edited_eval["chapter_barrier"],
        "chapter_barrier_restored": restored_eval["chapter_barrier"],
    }
    return {
        "before": before_eval,
        "edited": edited_eval,
        "restored": restored_eval,
        "checks": checks,
        "edited_sha256": hashlib.sha256(edited_source).hexdigest(),
        "restored_sha256": hashlib.sha256(restored_source).hexdigest(),
        "passes": all(checks.values()),
    }


def _capture(env, work, output, stage):
    directory = Path(work) / f"chapter-identity-{stage}"
    directory.mkdir()
    subprocess.run(["weston-screenshooter"], cwd=directory, env=env, check=True, timeout=10)
    screenshot, = directory.glob("*.png")
    shutil.copyfile(screenshot, Path(output).with_name(f"{Path(output).stem}-{stage}.png"))


def check(env, input_event, source_path, pid, output, probe_path, work):
    source_path = Path(source_path)
    original = source_path.read_bytes()

    def probe():
        result = subprocess.run(
            ["/usr/bin/python3", str(probe_path), str(pid)], env=env,
            capture_output=True, text=True, timeout=30,
        )
        if result.returncode:
            raise RuntimeError(result.stderr)
        return json.loads(result.stdout)["nodes"]

    def key(code, control=False):
        if control:
            input_event("key", 29, 1)
        input_event("key", code, 1)
        input_event("key", code, 0)
        if control:
            input_event("key", 29, 0)

    before = probe()
    evaluate_snapshot(before)
    _capture(env, work, output, "before")

    subprocess.run(
        ["wl-copy", "--seat", "tachyon-test", "--type", "text/plain"],
        input=TARGET, env=env, text=True, check=True, timeout=5,
    )
    key(33, control=True)  # Ctrl+F
    key(47, control=True)  # Ctrl+V
    time.sleep(0.3)
    key(1)  # Escape keeps the document match selected.
    key(106)  # Right collapses to the match end.
    key(45)  # X
    deadline = time.monotonic() + 6
    while source_path.read_bytes() == original and time.monotonic() < deadline:
        time.sleep(0.05)
    edited_source = source_path.read_bytes()
    if edited_source == original:
        raise RuntimeError("Native target edit did not reach autosave")
    time.sleep(0.8)
    edited = probe()
    _capture(env, work, output, "edited")

    key(44, control=True)  # Ctrl+Z
    deadline = time.monotonic() + 6
    while source_path.read_bytes() != original and time.monotonic() < deadline:
        time.sleep(0.05)
    restored_source = source_path.read_bytes()
    restored = probe()
    _capture(env, work, output, "restored")
    result = evaluate_transition(
        before, edited, restored, original, edited_source, restored_source,
    )
    result.update(
        fixture=source_path.name,
        original_sha256=hashlib.sha256(original).hexdigest(),
        source_unchanged=restored_source == original,
    )
    result["passes"] = result["passes"] and result["source_unchanged"]
    return result
