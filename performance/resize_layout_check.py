"""Native resize oracle for automatic editorial composition.

The app-side resize commands exist only in the ``layout-validation`` feature.
This module drives those commands through the isolated Wayland seat and uses
the canonical AT-SPI tree as the geometry/identity oracle.
"""

import hashlib
from dataclasses import dataclass
import json
import math
from pathlib import Path
import shutil
import subprocess
import time


ANCHOR = "3. Architecture and persistence"
PEERS = ("Document core", "Document view", "Application")
EDIT_PARAGRAPH = (
    "Measured shaping, automatic arrangements, hit testing, virtualization, "
    "tables, HTML, and formulas."
)
MARKDOWN_ESCAPABLE_BYTES = frozenset(b'!"#$%&\'()*+,-./:;<=>?@[\\]^_`{|}~')


@dataclass(frozen=True)
class Scenario:
    anchor: str = ANCHOR
    peers: tuple = PEERS
    edit_paragraph: str = EDIT_PARAGRAPH
    edit_search: str = "Measured shaping"
    peer_roles: tuple = ()
    # Exact authored plain paragraph, when soft source breaks differ from its
    # projected accessible text. Never normalize the rest of the saved file.
    edit_source: str | None = None
    wide_key: int = 68  # F10; F5 gives recent full-reading-width pairs room.
    height_stacks: bool = True
    wide_row: bool = True
    escape_edit: bool = True
    reading_setup_tail: str | None = None
    rich_timeline: bool = False
    paired_records: bool = False


TECHNICAL = Scenario(
    "Configuration options", ("Configuration options", "Configuration file"),
    "Save these values in your project configuration before opening the document.",
    "Save these values",
)


def scenario_for_fixture(name, target="default", zoom=1):
    if target.startswith("record-"):
        if (name != "122-paired-records.md" or target not in ("record-body", "record-header")
                or zoom not in (1, 1.5, 2)):
            raise ValueError("Record targets require fixture122 at 100, 150 or 200% text")
        source = (Path(__file__).parent / "layout-fixtures" / name).read_text()
        row, = (line for line in source.splitlines() if line.startswith("| Review bundle |"))
        text = row.split("|")[2].strip() if target == "record-body" else "Steward"
        return Scenario("Package directory", ("Field archive", "Review bundle"), text,
                        "Collects unresolved questions" if target == "record-body" else text,
                        ("table cell", "table cell"), wide_key=63, wide_row=zoom == 1,
                        height_stacks=zoom != 1, escape_edit=False,
                        reading_setup_tail="Following work", paired_records=True)
    if target.startswith("timeline-"):
        if (name != "119-rich-timelines.md"
                or target not in ("timeline-body", "timeline-code", "timeline-table", "timeline-nested")
                or zoom not in (1, 1.5, 2)):
            raise ValueError("Timeline targets require fixture119 at 100, 150 or 200% text")
        source = (Path(__file__).parent / "layout-fixtures" / name).read_text()
        dates = tuple(line.removeprefix("- ").replace("**", "")
                      for line in source.splitlines() if line.startswith("- **2026-"))
        markers = {
            "timeline-body": "Keep the exact command",
            "timeline-code": "tachyon field-notes.md",
            "timeline-table": "Original Markdown bytes retained",
            "timeline-nested": "Preserve nested qualifications",
        }
        marker = markers[target]
        line, = (line for line in source.splitlines() if marker in line)
        text = marker if target == "timeline-table" else line.strip().removeprefix("- ")
        return Scenario("Release notes", dates, text, marker, ("paragraph",) * 3,
                        wide_key=63, wide_row=False,
                        escape_edit=target in ("timeline-body", "timeline-nested"),
                        reading_setup_tail="Compact milestones", rich_timeline=True)
    if target != "default":
        if (name != "03-technical-reference.md"
                or target not in ("specification-table", "specification-code")
                or zoom not in (1, 1.5, 2)):
            raise ValueError("Specification targets require fixture03 at 100, 150 or 200% text")
        source = (Path(__file__).parent / "layout-fixtures" / name).read_text()
        code = source.split("```json\n", 1)[1].split("```", 1)[0]
        code_edit = target == "specification-code"
        return Scenario("Parameters", ("Parameters table", code),
                        code if code_edit else "Human-readable document title",
                        "A quieter place to think" if code_edit else "Human-readable document title",
                        ("table", "static"), wide_key=63, wide_row=zoom == 1,
                        escape_edit=False, reading_setup_tail="A sequence with an example")
    if name == "01-field-notes.md":
        source = (Path(__file__).parent / "layout-fixtures" / name).read_text()
        note = source.split("> [!NOTE]\n> ", 1)[1].split("\n", 1)[0]
        tip = source.split("> [!TIP]\n> ", 1)[1].split("\n", 1)[0]
        return Scenario("Details worth keeping", (note, tip), note, "A presentation choice",
                        ("notification", "notification"), wide_key=63, height_stacks=False)
    if name == "03-technical-reference.md":
        return Scenario("Configuration", ("Configuration", "Comparison across environments"),
                        "more than 60 fps", "more than 60 fps", ("heading", "heading"), wide_key=63)
    if name == "118-opening-overview.md":
        blocks = (Path(__file__).parent / "layout-fixtures" / name).read_text().split("\n\n")
        lead, overview = (block.replace("\n", " ") for block in blocks[1:3])
        return Scenario(blocks[0].removeprefix("# "), (lead, overview),
                        overview, "The current implementation", ("paragraph", "paragraph"), blocks[2])
    if name in ("116-code-led-explanations.md", "117-table-led-resize.md"):
        source = (Path(__file__).parent / "layout-fixtures" / name).read_text()
        table = name == "117-table-led-resize.md"
        if table:
            heading, _, following = source.split("\n\n", 2)
            technical = heading.removeprefix("## ") + " table"
        else:
            heading, remaining = source.split("\n\n```toml\n", 1)
            code, following = remaining.split("\n```\n\n", 1)
            technical = code + "\n"
        authored = following.split("\n\n", 1)[0]
        prose = authored.replace("\n", " ")
        return Scenario(heading.removeprefix("## "), (technical, prose),
                        prose, prose.split(".")[0], ("table" if table else "static", "paragraph"), authored)
    if name in ("106-extended-table.md", "107-extended-code.md"):
        # These explicit specimens have a heading, one complete explanation,
        # then technical content. Read their literal text, not duplicated copies.
        blocks = (Path(__file__).parent / "layout-fixtures" / name).read_text().split("\n\n")
        table = name == "106-extended-table.md"
        technical = "Retained document settings table" if table else blocks[2].removeprefix("```rust\n").removesuffix("\n```") + "\n"
        return Scenario(blocks[0].removeprefix("## "), (blocks[1], technical),
                        blocks[1], blocks[1].split(".")[0],
                        ("paragraph", "table" if table else "static"))
    scenarios = {"47-editorial-composition.md": Scenario(),
                 "79-technical-sections.md": TECHNICAL}
    if name not in scenarios:
        raise ValueError(f"No resize scenario for {name!r}")
    return scenarios[name]


def _one(nodes, role, name):
    matches = [node for node in nodes if node["role"] == role and node["name"] == name]
    if len(matches) != 1:
        raise RuntimeError(f"Expected one {role} named {name!r}, found {len(matches)}")
    return matches[0]


def editor_extent(snapshot, dimension):
    """A replacement tree may briefly contain its window but no editor yet."""
    nodes = snapshot["nodes"]
    if not any(node["role"] == "entry" and node["name"] == "Markdown document editor" for node in nodes):
        return None
    return _one(nodes, "entry", "Markdown document editor")["bounds"][dimension]


def _relative_y(nodes, name):
    viewport = _one(nodes, "entry", "Markdown document editor")["bounds"]
    heading = _one(nodes, "heading", name)["bounds"]
    return heading["y"] - viewport["y"]


def _heading_identities(nodes):
    return [(node["path"], node["name"]) for node in nodes if node["role"] == "heading"]


def _visible_heading_bounds(nodes):
    viewport = _one(nodes, "entry", "Markdown document editor")["bounds"]
    top = viewport["y"]
    bottom = top + viewport["height"]
    return [(node["name"], node["bounds"]) for node in nodes
            if node["role"] == "heading"
            and node["bounds"]["y"] + node["bounds"]["height"] >= top
            and node["bounds"]["y"] <= bottom]


def _peer_geometry(nodes, peers=PEERS, roles=()):
    if roles and len(roles) != len(peers):
        raise ValueError("Every mixed-content peer needs a semantic role")
    records = [_one(nodes, role, name) for role, name in zip(roles or ("heading",) * len(peers), peers)]
    bounds = [record["bounds"] for record in records]
    # Technical content has an internal code header/table cell inset. Its body
    # overlaps the explanation's vertical band, not the heading baseline.
    aligned = (max(bound["y"] for bound in bounds) < min(bound["y"] + bound["height"] for bound in bounds)
               if roles else max(bound["y"] for bound in bounds) - min(bound["y"] for bound in bounds) <= 2)
    row = (aligned
           and all(left["x"] + left["width"] <= right["x"]
                   for left, right in zip(bounds, bounds[1:])))
    stack = (all(upper["y"] + upper["height"] < lower["y"]
                 for upper, lower in zip(bounds, bounds[1:]))
             and max(bound["x"] for bound in bounds) - min(bound["x"] for bound in bounds) <= 2)
    return {"bounds": bounds, "row": row, "stack": stack,
            "identities": [(record["role"], record["path"]) for record in records]}


def evaluate_reading(before, narrow, narrow_again, restored, scenario=Scenario(), scale=120, axis="width"):
    if scale <= 0:
        raise ValueError("Display scale must be positive")
    before_nodes = before["nodes"]
    narrow_nodes = narrow["nodes"]
    repeat_nodes = narrow_again["nodes"]
    restored_nodes = restored["nodes"]
    editor_widths = [
        _one(nodes, "entry", "Markdown document editor")["bounds"]["width"]
        for nodes in (before_nodes, narrow_nodes, repeat_nodes, restored_nodes)
    ]
    editor_heights = [_one(nodes, "entry", "Markdown document editor")["bounds"]["height"]
                      for nodes in (before_nodes, narrow_nodes, repeat_nodes, restored_nodes)]
    dimensions = editor_heights if axis == "height" else editor_widths
    anchor_offsets = [
        _relative_y(nodes, scenario.anchor)
        for nodes in (before_nodes, narrow_nodes, repeat_nodes, restored_nodes)
    ]
    identities = [_heading_identities(nodes)
                  for nodes in (before_nodes, narrow_nodes, repeat_nodes, restored_nodes)]
    wide_peer = _peer_geometry(before_nodes, scenario.peers, scenario.peer_roles)
    narrow_peer = _peer_geometry(narrow_nodes, scenario.peers, scenario.peer_roles)
    restored_peer = _peer_geometry(restored_nodes, scenario.peers, scenario.peer_roles)
    narrow_heading_bounds = [node["bounds"] for node in narrow_nodes if node["role"] == "heading"]
    repeated_heading_bounds = [node["bounds"] for node in repeat_nodes
                               if node["role"] == "heading"]
    narrow_visible_headings = _visible_heading_bounds(narrow_nodes)
    repeated_visible_headings = _visible_heading_bounds(repeat_nodes)
    repeated_geometry = narrow_visible_headings == repeated_visible_headings
    checks = {
        "window_became_short" if axis == "height" else "window_became_narrow": dimensions[1] <= dimensions[0] - 100,
        "window_restored_tall" if axis == "height" else "window_restored_wide": dimensions[3] >= dimensions[1] + 100,
        "reading_anchor_stable": (max(anchor_offsets) - min(anchor_offsets)) * scale / 120 <= 1,
        "heading_identities_stable": all(identity == identities[0] for identity in identities[1:]),
        "wide_peer_row" if scenario.wide_row else "wide_peer_stack": wide_peer["row" if scenario.wide_row else "stack"],
        "short_peer_row" if axis == "height" and not scenario.height_stacks else "narrow_peer_stack":
            narrow_peer["row"] if axis == "height" and not scenario.height_stacks else narrow_peer["stack"],
        "restored_peer_row" if scenario.wide_row else "restored_peer_stack": restored_peer["row" if scenario.wide_row else "stack"],
        "peer_identities_stable": wide_peer["identities"] == narrow_peer["identities"] == restored_peer["identities"],
        "duplicate_resize_stable": repeated_geometry,
    }
    if axis == "height":
        checks["width_unchanged"] = max(editor_widths) - min(editor_widths) <= 1
    if scenario.rich_timeline:
        checks.update(_rich_timeline_checks((before_nodes, narrow_nodes, repeat_nodes, restored_nodes)))
    if scenario.paired_records:
        checks.update(_record_checks((before_nodes, narrow_nodes, repeat_nodes, restored_nodes)))
    return {
        "editor_widths": editor_widths,
        "editor_heights": editor_heights,
        "anchor_offsets": anchor_offsets,
        "anchor_displacement_physical_px": (max(anchor_offsets) - min(anchor_offsets)) * scale / 120,
        "wide_peer": wide_peer,
        "narrow_peer": narrow_peer,
        "restored_peer": restored_peer,
        "narrow_heading_bounds": narrow_heading_bounds,
        "repeated_heading_bounds": repeated_heading_bounds,
        "narrow_visible_headings": narrow_visible_headings,
        "repeated_visible_headings": repeated_visible_headings,
        "checks": checks,
        "passes": all(checks.values()),
    }


def evaluate_editing(before, after, state_before, state_after,
                     original_source, edited_source, restored_source, scenario=Scenario(), axis="width"):
    before_nodes = before["nodes"]
    after_nodes = after["nodes"]
    before_editor = _one(before_nodes, "entry", "Markdown document editor")
    after_editor = _one(after_nodes, "entry", "Markdown document editor")
    marker = scenario.edit_paragraph.encode()
    semantic_edited = _unescape_markdown_punctuation(edited_source) if scenario.escape_edit else edited_source
    insertion_offset = next(
        (offset for offset in range(len(marker) + 1)
         if semantic_edited.count(marker[:offset] + b"x" + marker[offset:]) == 1),
        None,
    )
    authored_marker = (scenario.edit_source or scenario.edit_paragraph).encode()
    target_start = original_source.find(authored_marker)
    insertion = ((target_start + insertion_offset, "x")
                 if target_start >= 0 and insertion_offset is not None else None)
    insertion_in_target = original_source.count(authored_marker) == 1 and insertion is not None
    edited_peers = tuple(
        peer[:insertion_offset] + "x" + peer[insertion_offset:]
        if peer == scenario.edit_paragraph and insertion_offset is not None else peer
        for peer in scenario.peers)
    checks = {
        "window_became_short" if axis == "height" else "window_became_narrow": after_editor["bounds"][axis] <= before_editor["bounds"][axis] - 100,
        "editor_identity_stable": before_editor["path"] == after_editor["path"],
        "editor_focus_stable": all("focused" in editor["states"] for editor in (before_editor, after_editor)),
        "caret_stable": (state_before["selection_start"] == state_before["selection_end"]
                          and state_before["selection_start"] == state_after["selection_start"]
                          and state_before["selection_end"] == state_after["selection_end"]
                          and state_before["focused"] and state_after["focused"]),
        "edited_source_stable": edited_source != original_source,
        "insertion_in_target": insertion_in_target,
        "wide_peer_row" if scenario.wide_row else "wide_peer_stack":
            _peer_geometry(before_nodes, edited_peers, scenario.peer_roles)["row" if scenario.wide_row else "stack"],
        "short_peer_row" if axis == "height" and not scenario.height_stacks else "narrow_peer_stack":
            _peer_geometry(after_nodes, edited_peers, scenario.peer_roles)[
                "row" if axis == "height" and not scenario.height_stacks else "stack"],
        "heading_identities_stable": _heading_identities(before_nodes) == _heading_identities(after_nodes),
        "undo_source_exact": restored_source == original_source,
    }
    if axis == "height":
        checks["width_unchanged"] = abs(after_editor["bounds"]["width"] - before_editor["bounds"]["width"]) <= 1
    if scenario.rich_timeline or scenario.paired_records:
        checks.update(_rich_timeline_checks((before_nodes, after_nodes)) if scenario.rich_timeline
                      else _record_checks((before_nodes, after_nodes)))
        checks["caret_line_visible"] = all(caret_line_visible(state) for state in (state_before, state_after))
        checks["caret_visible"] = all(caret_visible(state) for state in (state_before, state_after))
    if scenario.peer_roles:
        # Allow canonical escaping only inside the exact edited text target
        # (paragraph or cell); code stays literal. All surrounding bytes must
        # remain unchanged in either case.
        expected = None
        if insertion_offset is not None:
            value = marker[:insertion_offset] + b"x" + marker[insertion_offset:]
            escaped = b"".join((b"\\" if byte in MARKDOWN_ESCAPABLE_BYTES and byte != ord('|') else b"")
                               + bytes([byte]) for byte in value)
            expected = original_source.replace(authored_marker, escaped if scenario.escape_edit else value, 1)
        checks["edited_source_exact"] = edited_source == expected
    return {
        "editor_before": before_editor,
        "editor_after": after_editor,
        "state_before": state_before,
        "state_after": state_after,
        "insertion": insertion,
        "edited_sha256": hashlib.sha256(edited_source).hexdigest(),
        "restored_sha256": hashlib.sha256(restored_source).hexdigest(),
        "checks": checks,
        "passes": all(checks.values()),
    }


def evaluate_editing_restoration(before, restored, state_before, state_restored,
                                 edited_source, restored_edited_source, scenario, axis="width", frozen=None):
    before_nodes, restored_nodes = before["nodes"], restored["nodes"]
    before_editor = _one(before_nodes, "entry", "Markdown document editor")
    restored_editor = _one(restored_nodes, "entry", "Markdown document editor")
    checks = {
        "restored_window_dimensions": all(abs(before_editor["bounds"][dimension] - restored_editor["bounds"][dimension]) <= 1
                                           for dimension in ("width", "height")),
        "restored_editor_identity": before_editor["path"] == restored_editor["path"],
        "restored_editor_focus": "focused" in restored_editor["states"] and state_restored["focused"],
        "restored_caret_offset": (state_before["selection_start"] == state_before["selection_end"]
                                  == state_restored["selection_start"] == state_restored["selection_end"]),
        "restored_caret_visible": caret_visible(state_restored),
        "restored_edit_exact": restored_edited_source == edited_source,
        "restored_peer_row" if scenario.wide_row else "restored_peer_stack":
            _peer_geometry(restored_nodes, scenario.peers, scenario.peer_roles)["row" if scenario.wide_row else "stack"],
        "restored_heading_identities": _heading_identities(before_nodes) == _heading_identities(restored_nodes),
    }
    if scenario.rich_timeline:
        checks.update({"restored_" + name: result for name, result in
                       _rich_timeline_checks((before_nodes, restored_nodes)).items()})
    if scenario.paired_records:
        if axis == "width":
            # A focused stack retains its narrower measure when the window
            # grows. Recomposition is required after leaving the edited block,
            # not while its caret is still held in that frozen arrangement.
            del checks["restored_peer_row" if scenario.wide_row else "restored_peer_stack"]
            frozen_nodes = frozen["nodes"]
            frozen_peer = _peer_geometry(frozen_nodes, scenario.peers, scenario.peer_roles)
            restored_peer = _peer_geometry(restored_nodes, scenario.peers, scenario.peer_roles)
            frozen_editor = _one(frozen_nodes, "entry", "Markdown document editor")["bounds"]
            checks["restored_focused_record_measure"] = all(
                a["width"] == b["width"] and a["height"] == b["height"]
                and a["x"] - frozen_editor["x"] == b["x"] - restored_editor["bounds"]["x"]
                for a, b in zip(frozen_peer["bounds"], restored_peer["bounds"]))
            checks["restored_focused_record_stack"] = frozen_peer["stack"] and restored_peer["stack"]
        checks.update({"restored_" + name: result for name, result in
                       _record_checks((before_nodes, restored_nodes)).items()})
    return {"checks": checks, "passes": all(checks.values()), "axis": axis,
            "editor": restored_editor, "state": state_restored,
            "edited_sha256": hashlib.sha256(restored_edited_source).hexdigest()}


def caret_visible(state):
    if not caret_line_visible(state):
        return False
    caret, viewport = state["caret_bounds"], state["viewport_bounds"]
    return (viewport[2] > viewport[0] and caret[2] > caret[0]
            and caret[0] >= viewport[0] - 1 and caret[2] <= viewport[2] + 1)


def caret_line_visible(state):
    """Compare native caret/viewport bounds in their shared window coordinates."""
    caret = state.get("caret_bounds")
    viewport = state.get("viewport_bounds")
    if not caret or not viewport or len(caret) != 4 or len(viewport) != 4:
        return False
    return (all(math.isfinite(value) for value in (*caret, *viewport))
            and viewport[3] > viewport[1] and caret[3] > caret[1]
            and caret[1] >= viewport[1] - 1 and caret[3] <= viewport[3] + 1)


def _rich_timeline_checks(snapshots):
    from reference_prose_flow_check import semantics
    from rich_timeline_check import event_support_geometry

    canonical = [semantics(nodes, 68) for nodes in snapshots]
    geometry = [event_support_geometry(nodes) for nodes in snapshots]
    return {
        "complete_timeline_semantics": all(len(tree) == 68 and tree == canonical[0] for tree in canonical),
        "supporting_content_inside_its_event": all(report["passes"] for report in geometry),
    }


def _record_checks(snapshots):
    from reference_prose_flow_check import semantics
    from paired_records_check import rows, record_geometry

    canonical = [semantics(nodes, 88) for nodes in snapshots]
    complete = all(len(tree) == 88 and tree == canonical[0] for tree in canonical)
    geometry = all(record_geometry(rows(nodes, "Package directory")[1:]) for nodes in snapshots)
    return {"complete_record_semantics": complete, "complete_record_geometry": bool(geometry)}


def _single_insertion(original, candidate):
    if len(candidate) != len(original) + 1:
        return None
    for offset in range(len(candidate)):
        if candidate[:offset] == original[:offset] and candidate[offset + 1:] == original[offset:]:
            return offset, candidate[offset]
    return None


def record_text_transition(before, after, old, new):
    """Only the edited cell and its paragraph may change their accessible names."""
    from reference_prose_flow_check import semantics
    from paired_records_check import rows, record_geometry

    expected = []
    changed = 0
    for record in semantics(before, 88):
        path, parent, role, name, description, actions = record
        if role in ("column header", "table cell", "paragraph") and name == old:
            name = new
            changed += 1
        expected.append((path, parent, role, name, description, actions))
    return (changed == 2 and len(expected) == 88 and semantics(after, 88) == expected
            and bool(record_geometry(rows(after, "Package directory")[1:])))


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


def _capture(env, work, output, stage):
    directory = work / f"resize-{stage}"
    directory.mkdir()
    subprocess.run(["weston-screenshooter"], cwd=directory, env=env, check=True, timeout=10)
    screenshot, = directory.glob("*.png")
    shutil.copyfile(screenshot, output.with_name(f"{output.stem}-{stage}.png"))


def committed_viewport_matches(reports, snapshot, expected_zoom=None):
    """Reject stale committed geometry even when the window itself resized."""
    committed = [report for report in reports if report.get("committed") is True]
    if not committed:
        return False
    report = committed[-1]
    viewport = _one(snapshot["nodes"], "entry", "Markdown document editor")["bounds"]
    zoom = report.get("text_zoom", 0)
    return (zoom > 0 and (expected_zoom is None or abs(zoom - expected_zoom) < 0.001)
            and report.get("planner_failed") is False
            and abs(report.get("canvas_width", -1000) * zoom - viewport["width"]) <= 1
            and abs(report.get("viewport_height", -1000) * zoom - viewport["height"]) <= 1)


def resize_keys(final_key, burst, wide_key=68):
    if wide_key not in (68, 63) or final_key not in (67, wide_key):
        raise ValueError("Resize target must be F9 or the scenario's wide key")
    opposite_key = wide_key if final_key == 67 else 67
    return [final_key, opposite_key] * 4 + [final_key] if burst else [final_key]


def prefixed_records(data, prefix):
    """Read complete LF-terminated records from a concurrently written log.

    Frame bytes before decoding so a partial UTF-8 character at the tail is
    harmless. Complete malformed records still fail; no evidence is skipped.
    """
    prefix = prefix.encode("ascii")
    return [json.loads(line[len(prefix):]) for line in data.split(b"\n")[:-1]
            if line.startswith(prefix)]


def check(mode, env, input_event, source_path, pid, output, probe_path, work, log_path, scale=120, axis="width", burst=False,
          target="default", zoom=1, continue_edit=False):
    scenario = scenario_for_fixture(source_path.name, target, zoom)
    if continue_edit and (mode != "editing" or not scenario.paired_records):
        raise ValueError("Continued editing requires the explicit paired-record editing scenario")
    original = source_path.read_bytes()

    def probe(scroll_to=None):
        command = ["/usr/bin/python3", str(probe_path), str(pid)]
        if scroll_to is not None:
            command.append(f"--scroll-to-name={scroll_to}")
        result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=30)
        if result.returncode:
            raise RuntimeError(result.stderr)
        return json.loads(result.stdout)

    def find_and_collapse(query):
        subprocess.run(["wl-copy", "--seat", "mineral-test", "--type", "text/plain"],
                       input=query, text=True, env=env, check=True, timeout=5)
        input_event("key", 29, 1)
        input_event("key", 33, 1)  # Ctrl+F
        input_event("key", 33, 0)
        input_event("key", 29, 0)
        time.sleep(0.1)
        input_event("key", 29, 1)
        input_event("key", 47, 1)  # Ctrl+V into Find
        input_event("key", 47, 0)
        input_event("key", 29, 0)
        time.sleep(0.2)
        input_event("key", 1, 1)  # Escape retains the document match.
        input_event("key", 1, 0)
        input_event("key", 106, 1)  # Collapse to the match end.
        input_event("key", 106, 0)

    bursts = []
    incomplete_resize_snapshots = 0

    def resize(key, dimension=axis, rapid=False):
        events = []
        start = time.monotonic()
        wide_key = scenario.wide_key if dimension == "width" else 68
        key = wide_key if key == 68 else key
        for target in resize_keys(key, rapid, wide_key):
            if dimension == "height":
                input_event("key", 42, 1)
            input_event("key", target, 1)
            input_event("key", target, 0)
            if dimension == "height":
                input_event("key", 42, 0)
            events.append({"key": target, "elapsed_ms": (time.monotonic() - start) * 1000})
        if rapid:
            bursts.append({"axis": dimension, "final_key": key, "events": events})

    def trace_reports():
        prefix = "MINERAL_LAYOUT_TRACE "
        return prefixed_records(log_path.read_bytes(), prefix)

    def report_editor_state():
        prefix = "MINERAL_RESIZE_STATE "
        before_count = len(prefixed_records(log_path.read_bytes(), prefix))
        input_event("key", 66, 1)  # F8
        input_event("key", 66, 0)
        deadline = time.monotonic() + 5
        while True:
            records = prefixed_records(log_path.read_bytes(), prefix)
            if len(records) > before_count:
                return records[-1]
            if time.monotonic() >= deadline:
                raise RuntimeError("Validation editor-state report did not arrive")
            time.sleep(0.02)

    def undo_to(expected):
        input_event("key", 29, 1)
        input_event("key", 44, 1)
        input_event("key", 44, 0)
        input_event("key", 29, 0)
        deadline = time.monotonic() + 5
        while source_path.read_bytes() != expected and time.monotonic() < deadline:
            time.sleep(0.05)
        return source_path.read_bytes() == expected

    def type_and_undo(stage, key, expected, old_name, new_name, baseline):
        before_trial = probe()
        state = report_editor_state()
        input_event("key", key, 1)
        input_event("key", key, 0)
        time.sleep(1.0)  # Let autosave and focused layout publication settle.
        after_trial = probe()
        typed = report_editor_state()
        _capture(env, work, output, stage)
        checks = {
            "exact_saved_target": source_path.read_bytes() == expected,
            "selected_cell_visible_before_typing": caret_visible(state),
            "caret_visible_after_typing": caret_visible(typed),
            "caret_advanced_one_character": (typed["focused"] and state["focused"]
                and state["selection_start"] == state["selection_end"]
                and typed["selection_start"] == typed["selection_end"] == state["selection_end"] + 1),
            "only_target_semantics_changed": record_text_transition(
                before_trial["nodes"], after_trial["nodes"], old_name, new_name),
            "exact_stepwise_undo": undo_to(baseline),
        }
        time.sleep(1.0)
        checks.update(_record_checks((before_trial["nodes"], probe()["nodes"])))
        return {"checks": checks, "passes": all(checks.values()), "before": state, "typed": typed,
                "expected_sha256": hashlib.sha256(expected).hexdigest()}

    def await_width(reference, narrower, dimension=axis):
        nonlocal incomplete_resize_snapshots
        deadline = time.monotonic() + 10
        while True:
            snapshot = probe()
            width = editor_extent(snapshot, dimension)
            if width is None:
                incomplete_resize_snapshots += 1
            elif (narrower and width <= reference - 100) or (not narrower and width >= reference + 100):
                time.sleep(1.0)
                settled = probe()
                extent = editor_extent(settled, dimension)
                if extent is not None and ((narrower and extent <= reference - 100)
                                          or (not narrower and extent >= reference + 100)):
                    return settled
                if extent is None:
                    incomplete_resize_snapshots += 1
            if time.monotonic() >= deadline:
                raise RuntimeError(f"Validation resize did not change editor {dimension} from {reference}: {width}")
            time.sleep(0.05)

    initial = probe()
    initial_width = _one(initial["nodes"], "entry", "Markdown document editor")["bounds"]["width"]
    resize(68, "width")  # Begin from an unambiguously wide composition.
    wide = await_width(initial_width, False, "width")
    if scenario.reading_setup_tail and mode == "reading":
        # ScrollIntoView may leave an already-visible heading at the bottom.
        # Reveal a later section first, then this one. Native accessible reveal
        # preserves selection and does not depend on compositor window origin.
        probe(scenario.reading_setup_tail)
    # Establish the reading position before making the window taller. Otherwise
    # the target may become visible near the bottom, and ScrollIntoView can
    # legitimately leave it there rather than make it the top reading anchor.
    probe(scenario.anchor if mode == "reading" or scenario.peer_roles else scenario.peers[1])
    if axis == "height":
        height = _one(wide["nodes"], "entry", "Markdown document editor")["bounds"]["height"]
        resize(68)
        await_width(height, False)
    before = probe()
    if scenario.reading_setup_tail and mode == "reading" and not 0 <= _relative_y(before["nodes"], scenario.anchor) <= 32 * zoom:
        raise RuntimeError("Reading setup did not place its heading near the viewport top")
    _capture(env, work, output, f"{mode}-wide")

    if mode == "editing":
        find_and_collapse(scenario.edit_search)
        input_event("key", 45, 1)  # X
        input_event("key", 45, 0)
        deadline = time.monotonic() + 5
        while source_path.read_bytes() == original and time.monotonic() < deadline:
            time.sleep(0.05)
        edited = source_path.read_bytes()
        before = probe()
        state_before = report_editor_state()
        _capture(env, work, output, "editing-typed-wide")

    before_width = _one(before["nodes"], "entry", "Markdown document editor")["bounds"][axis]
    resize(67, rapid=burst)  # F9
    narrow = await_width(before_width, True)
    narrow_committed = not burst or committed_viewport_matches(trace_reports(), narrow, zoom)
    _capture(env, work, output, f"{mode}-narrow")

    if mode == "reading":
        resize(67)
        time.sleep(1.0)
        narrow_again = probe()
        narrow_width = _one(narrow["nodes"], "entry", "Markdown document editor")["bounds"][axis]
        resize(68, rapid=burst)  # F10
        restored = await_width(narrow_width, False)
        _capture(env, work, output, "reading-restored-wide")
        report = evaluate_reading(before, narrow, narrow_again, restored, scenario, scale, axis)
        report["source_unchanged"] = source_path.read_bytes() == original
        report["passes"] = report["passes"] and report["source_unchanged"]
        if burst:
            report["checks"]["final_committed_viewport"] = committed_viewport_matches(trace_reports(), restored, zoom)
    else:
        edited_after_resize = source_path.read_bytes()
        state_after = report_editor_state()
        restoration = None
        if scenario.rich_timeline or scenario.paired_records:
            narrow_extent = _one(narrow["nodes"], "entry", "Markdown document editor")["bounds"][axis]
            resize(68, rapid=burst)
            restored = await_width(narrow_extent, False)
            state_restored = report_editor_state()
            _capture(env, work, output, "editing-restored-wide")
            restoration = evaluate_editing_restoration(before, restored, state_before, state_restored,
                                                        edited, source_path.read_bytes(), scenario, axis, frozen=narrow)
            restoration["checks"]["resize_did_not_mutate_edit"] = edited == edited_after_resize
            if burst:
                restoration["checks"]["restored_committed_viewport"] = committed_viewport_matches(trace_reports(), restored, zoom)
            if scenario.paired_records:
                if continue_edit:
                    old_name = scenario.edit_paragraph.replace(scenario.edit_search, scenario.edit_search + "x", 1)
                    new_name = scenario.edit_paragraph.replace(scenario.edit_search, scenario.edit_search + "xy", 1)
                    if edited.count(old_name.encode()) != 1:
                        raise RuntimeError("Continued edit target is not source-unique")
                    trials = {"continued_typing": type_and_undo("editing-continued", 21,
                        edited.replace(old_name.encode(), new_name.encode(), 1), old_name, new_name, edited)}
                    # Shift+Tab must move to the previous source cell, including
                    # the offscreen Repository header in a wide table.
                    input_event("key", 42, 1)
                    input_event("key", 15, 1)
                    input_event("key", 15, 0)
                    input_event("key", 42, 0)
                    time.sleep(1.0)
                    previous = "Repository" if target == "record-header" else "Review bundle"
                    trials["previous_cell"] = type_and_undo("editing-previous-cell", 44,
                        edited.replace(("| " + previous + " |").encode(), ("| z" + previous + " |").encode(), 1),
                        previous, "z" + previous, edited)
                    input_event("key", 15, 1)
                    input_event("key", 15, 0)
                    time.sleep(1.0)
                    trials["next_cell"] = type_and_undo("editing-next-cell", 17,
                        edited.replace(old_name.encode(), ("w" + old_name).encode(), 1), old_name, "w" + old_name, edited)
                    restoration["continued_editing"] = trials
                    restoration["checks"]["continued_typing_and_cell_navigation"] = all(
                        trial["passes"] for trial in trials.values())
                # Wayland does not expose global window origins. Use the
                # native Find path rather than treating window-local bounds
                # as physical screen coordinates in the desktop shell.
                find_and_collapse(scenario.anchor)
                time.sleep(1.0)
                released = probe()
                _capture(env, work, output, "editing-released-wide")
                geometry = _peer_geometry(released["nodes"], scenario.peers, scenario.peer_roles)
                restoration["checks"]["released_record_recomposition"] = geometry["row" if scenario.wide_row else "stack"]
                restoration["checks"]["released_edit_exact"] = source_path.read_bytes() == edited
                restoration["checks"].update({"released_" + name: value for name, value in
                    _record_checks((restored["nodes"], released["nodes"])).items()})
                restoration["released_peer"] = geometry
            restoration["passes"] = all(restoration["checks"].values())
        undo_to(original)
        restored_source = source_path.read_bytes()
        report = evaluate_editing(before, narrow, state_before, state_after,
                                  original, edited_after_resize, restored_source, scenario, axis)
        if restoration is not None:
            report["restoration"] = restoration
            report["checks"].update(restoration["checks"])
            report["passes"] = report["passes"] and restoration["passes"]

    if burst:
        report["checks"]["narrow_committed_viewport"] = narrow_committed
        report["passes"] = report["passes"] and all(report["checks"].values())
    report.update(mode=mode, axis=axis, burst=burst, resize_bursts=bursts, fixture=source_path.name,
                  target=target, text_zoom=zoom, continue_edit=continue_edit,
                  incomplete_resize_snapshots=incomplete_resize_snapshots,
                  original_sha256=hashlib.sha256(original).hexdigest())
    return report
