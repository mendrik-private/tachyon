"""Native AT-SPI oracle for measured automatic list, row, and table layouts."""

import hashlib
import json
from pathlib import Path
import shutil
import subprocess


LIST_GROUPS = {
    "directions": ["North", "South", "East", "West", "Above", "Below"],
    "features": [
        "Clarity: Find the main idea.",
        "Continuity: Keep the reader's place.",
        "Balance: Give ideas similar weight.",
        "Restraint: Decorate with purpose.",
        "Fidelity: Preserve the structure.",
        "Comfort: Leave room to think.",
    ],
    "uneven": [
        "A brief observation.",
        "Another short point.",
        ("This item contains a longer explanation about keeping a document readable. It "
         "includes several qualifications, an example of why content measurement matters, "
         "and a conclusion that belongs with the rest of the paragraph. The layout must "
         "preserve the complete explanation, without reducing its font size, hiding any "
         "text, or leaving five short items alongside a much taller card."),
        "A fourth observation.",
        "A fifth observation.",
        "A final short point.",
    ],
    "references": [
        "Initial state.",
        "The result from step 1 remains available.",
        "A separate conclusion.",
    ],
    "ten": ["Amber", "Birch", "Cedar", "Dune", "Elm", "Fern", "Grove", "Hazel", "Iris", "Juniper"],
}

ROW_HEADINGS = [
    "Measured document rows", "Starting a worker", "Review checks", "Content fidelity",
    "Readable measures", "Stable interaction", "Another example", "A separate section",
]
ROW_PEERS = ["Content fidelity", "Readable measures", "Stable interaction"]
ROW_CODE = {
    "Starting a worker": (
        "Read the saved configuration before starting the worker.",
        "let configuration = read_configuration_from_workspace(&workspace_directory)?;\n"
        "start_worker(configuration);\n",
        ("The next paragraph belongs below the complete explanation and code example, "
         "regardless of which side is taller."),
    ),
    "Another example": (
        "The short example can share a row with its explanation when both have a comfortable width.",
        "tachyon document.md\n",
        None,
    ),
}

TABLES = {
    "Runtime properties table": [
        ["Property", "Value"], ["Timeout", "30 s"], ["Workers", "4"], ["Cache", "64 MiB"],
    ],
    "Environment comparison table": [
        ["Environment", "Service endpoint", "Retries"],
        ["Development", "https://development.example.test/service", "3"],
        ["Staging", "https://staging.example.test/service", "2"],
        ["Production", "https://production.example.test/service", "5"],
    ],
    "Explanation and table table": [
        ["Setting", "Default", "Purpose"],
        ["Workers", "4", "Limit concurrent jobs"],
        ["Timeout", "30 s", "Bound waiting time"],
        ["Retries", "3", "Retry transient failures"],
    ],
    "Unbroken identifiers table": [
        ["Identifier", "Description"],
        ["configuration_schema_revision_identifier",
         "Keep this identifier complete and horizontally accessible on a narrow viewport."],
        ["id", "Compact labels should not receive the same minimum width as long identifiers."],
    ],
}


def _one(nodes, role, name):
    matches = [(index, node) for index, node in enumerate(nodes)
               if node["role"] == role and node["name"] == name]
    if len(matches) != 1:
        roles = [node["role"] for node in nodes if node["name"] == name]
        raise RuntimeError(f"Expected one {role} named {name!r}, found {len(matches)}; roles={roles!r}")
    return matches[0]


def _content_node(nodes, name):
    matches = [(index, node) for index, node in enumerate(nodes)
               if node["name"] == name and node["role"] in {"paragraph", "static"}]
    if len(matches) != 1:
        raise RuntimeError(f"Expected one accessible content node named {name!r}, found {len(matches)}")
    return matches[0]


def _bounds(records):
    result = [node.get("bounds") for _, node in records]
    if any(not bound or bound["width"] <= 0 or bound["height"] <= 0 for bound in result):
        raise RuntimeError(f"Accessible content lost non-empty geometry: {result!r}")
    return result


def _canonical_list_parent(nodes, records):
    list_parents = []
    for _, record in records:
        item_index = record["parent"]
        if item_index is None or nodes[item_index]["role"] != "list item":
            raise RuntimeError(f"List content {record['name']!r} lost its list-item parent")
        list_index = nodes[item_index]["parent"]
        if list_index is None or nodes[list_index]["role"] != "list":
            raise RuntimeError(f"List item {record['name']!r} lost its list parent")
        list_parents.append(list_index)
    if len(set(list_parents)) != 1:
        raise RuntimeError(f"One authored list split across semantic containers: {list_parents!r}")
    return list_parents[0]


def _cluster(values, tolerance=4):
    clusters = []
    for value in sorted(values):
        if not clusters or value - clusters[-1][-1] > tolerance:
            clusters.append([value])
        else:
            clusters[-1].append(value)
    return [round(sum(group) / len(group)) for group in clusters]


def _visual_order(bounds, tolerance=4):
    """Return row-major source order and the observed row/column counts."""
    rows = _cluster([bound["y"] for bound in bounds], tolerance)
    coordinates = []
    for bound in bounds:
        row = min(range(len(rows)), key=lambda index: abs(rows[index] - bound["y"]))
        coordinates.append((row, bound["x"]))
    row_major = all(current > previous for previous, current in zip(coordinates, coordinates[1:]))
    columns = max((sum(row == candidate for row, _ in coordinates) for candidate in range(len(rows))), default=0)
    return {"rows": len(rows), "columns": columns, "row_major": row_major,
            "coordinates": coordinates}


def _nonoverlapping(bounds):
    for index, left in enumerate(bounds):
        for right in bounds[index + 1:]:
            separated = (left["x"] + left["width"] <= right["x"] + 1
                         or right["x"] + right["width"] <= left["x"] + 1
                         or left["y"] + left["height"] <= right["y"] + 1
                         or right["y"] + right["height"] <= left["y"] + 1)
            if not separated:
                return False
    return True


def _stacked(bounds, tolerance=6):
    return (max(bound["x"] for bound in bounds) - min(bound["x"] for bound in bounds) <= tolerance
            and all(left["y"] + left["height"] <= right["y"] + 1
                    for left, right in zip(bounds, bounds[1:])))


def evaluate_lists(nodes, profile):
    if profile not in {"wide", "medium", "narrow"}:
        raise RuntimeError(f"Unknown adaptive profile {profile!r}")
    results = {}
    for group, names in LIST_GROUPS.items():
        records = [_content_node(nodes, name) for name in names]
        list_parent = _canonical_list_parent(nodes, records)
        indices = [index for index, _ in records]
        bounds = _bounds(records)
        visual = _visual_order(bounds)
        exact_source_order = indices == sorted(indices)
        nonoverlap = _nonoverlapping(bounds)
        if not exact_source_order or not visual["row_major"] or not nonoverlap:
            raise RuntimeError(f"List {group} lost source order or legal geometry: {visual!r}")
        should_stack = group in {"uneven", "references", "ten"} or profile == "narrow"
        if should_stack and not _stacked(bounds):
            raise RuntimeError(f"List {group} should be a readable vertical stack: {bounds!r}")
        if not should_stack:
            allowed = ({3} if profile == "wide"
                       else ({2, 3} if group == "directions" else {1, 2, 3}))
            if visual["columns"] not in allowed or visual["rows"] < 2:
                raise RuntimeError(
                    f"List {group} did not choose a measured {sorted(allowed)}-column grid: {visual!r}"
                )
        results[group] = dict(bounds=bounds, source_indices=indices, list_parent=list_parent,
                              exact_source_order=exact_source_order, nonoverlap=nonoverlap,
                              stacked=_stacked(bounds), **visual)

    nested = [_content_node(nodes, name) for name in (
        "Open the workspace.", "Keep the original source file available.",
        "Verify the document title.", "Run the verification command.",
        "cargo test --locked -p document-view\n", "Review the outcome.",
    )]
    nested_indices = [index for index, _ in nested]
    if nested_indices != sorted(nested_indices):
        raise RuntimeError(f"Nested instruction traversal changed: {nested_indices!r}")
    nested_bounds = _bounds(nested)
    if any(left["y"] > right["y"] for left, right in zip(nested_bounds, nested_bounds[1:])):
        raise RuntimeError(f"Nested instruction visual order changed: {nested_bounds!r}")
    results["nested"] = dict(source_indices=nested_indices, bounds=nested_bounds,
                             exact_source_order=True)
    return {"kind": "lists", "profile": profile, "groups": results, "passes": True}


def _same_row(bounds, tolerance=4):
    return max(bound["y"] for bound in bounds) - min(bound["y"] for bound in bounds) <= tolerance


def _side_by_side(bounds):
    return all(left["x"] + left["width"] <= right["x"] + 1
               for left, right in zip(bounds, bounds[1:]))


def evaluate_rows(nodes, profile):
    headings = [node["name"] for node in nodes if node["role"] == "heading"]
    if headings != ROW_HEADINGS:
        raise RuntimeError(f"Row fixture heading traversal changed: {headings!r}")
    peers = [_one(nodes, "heading", name) for name in ROW_PEERS]
    peer_bounds = _bounds(peers)
    if profile == "wide":
        if not _same_row(peer_bounds) or not _nonoverlapping(peer_bounds):
            raise RuntimeError(f"Matched sibling sections did not form a legal row: {peer_bounds!r}")
    elif not _stacked(peer_bounds):
        raise RuntimeError(f"Matched sibling sections did not stack at {profile} width: {peer_bounds!r}")

    examples = {}
    for heading, (paragraph_name, code_name, following_name) in ROW_CODE.items():
        paragraph = _one(nodes, "paragraph", paragraph_name)
        code = _one(nodes, "static", code_name)
        indices = [paragraph[0], code[0]]
        bounds = _bounds([paragraph, code])
        if indices != sorted(indices) or not _nonoverlapping(bounds):
            raise RuntimeError(f"{heading} explanation/code lost order or overlap: {bounds!r}")
        wide_pair_expected = profile == "wide" and heading == "Another example"
        if wide_pair_expected and not _side_by_side(bounds):
            raise RuntimeError(f"{heading} explanation/code did not share a row: {bounds!r}")
        if profile == "wide" and heading == "Starting a worker" and not _stacked(bounds):
            raise RuntimeError(f"{heading} long example did not retain its readable stack: {bounds!r}")
        if profile != "wide" and not _stacked(bounds):
            raise RuntimeError(f"{heading} explanation/code did not stack: {bounds!r}")
        if following_name:
            following = _one(nodes, "paragraph", following_name)
            bottom = max(bound["y"] + bound["height"] for bound in bounds)
            if following[1]["bounds"]["y"] < bottom:
                raise RuntimeError(f"Content following {heading} starts inside its adaptive row")
        examples[heading] = dict(bounds=bounds, source_indices=indices,
                                 side_by_side=_side_by_side(bounds), stacked=_stacked(bounds))

    separate = _one(nodes, "heading", "A separate section")
    prior_code = _one(nodes, "static", ROW_CODE["Another example"][1])
    if separate[0] <= prior_code[0] or separate[1]["bounds"]["y"] <= prior_code[1]["bounds"]["y"]:
        raise RuntimeError("The thematic boundary no longer separates the final section")
    return {"kind": "rows", "profile": profile, "headings": headings,
            "peer_bounds": peer_bounds, "peer_row": _same_row(peer_bounds),
            "peer_stack": _stacked(peer_bounds), "examples": examples,
            "thematic_boundary_preserved": True, "passes": True}


def _table_shape(nodes, name, expected):
    table_index, table = _one(nodes, "table", name)
    rows = [(index, node) for index, node in enumerate(nodes)
            if node["parent"] == table_index and node["role"] == "table row"]
    if len(rows) != len(expected):
        raise RuntimeError(f"{name} has {len(rows)} rows, expected {len(expected)}")
    observed = []
    for ordinal, ((row_index, _), expected_names) in enumerate(zip(rows, expected)):
        cells = [node for node in nodes if node["parent"] == row_index]
        expected_role = "column header" if ordinal == 0 else "table cell"
        names = [cell["name"] for cell in cells]
        if names != expected_names or any(cell["role"] != expected_role for cell in cells):
            raise RuntimeError(f"{name} row {ordinal + 1} lost exact cells/order: {names!r}")
        observed.append(names)
    return table_index, table, observed


def evaluate_tables(nodes, profile):
    records = {}
    for name, expected in TABLES.items():
        index, table, observed = _table_shape(nodes, name, expected)
        records[name] = {"source_index": index, "bounds": table["bounds"], "cells": observed}
    runtime = records["Runtime properties table"]
    environment = records["Environment comparison table"]
    pair = [runtime["bounds"], environment["bounds"]]
    if profile == "wide":
        if not _same_row(pair) or not _nonoverlapping(pair):
            raise RuntimeError(f"Unequal adjacent tables did not form a legal wide row: {pair!r}")
        if runtime["bounds"]["width"] >= environment["bounds"]["width"]:
            raise RuntimeError(f"Intrinsic table widths were flattened instead of auto-fitted: {pair!r}")
    elif not _stacked(pair):
        raise RuntimeError(f"Adjacent tables did not stack at {profile} width: {pair!r}")
    if [record["source_index"] for record in records.values()] != sorted(
            record["source_index"] for record in records.values()):
        raise RuntimeError("Table traversal no longer follows authored source order")
    return {"kind": "tables", "profile": profile, "tables": records,
            "pair_bounds": pair, "pair_row": _same_row(pair),
            "pair_stack": _stacked(pair), "passes": True}


def check(kind, profile, env, source_path, pid, output, probe_path, work):
    source_path = Path(source_path)
    original = source_path.read_bytes()
    probe = subprocess.run(
        ["/usr/bin/python3", str(probe_path), str(pid)], env=env,
        capture_output=True, text=True, timeout=30,
    )
    if probe.returncode:
        raise RuntimeError(probe.stderr)
    nodes = json.loads(probe.stdout)["nodes"]
    evaluators = {"lists": evaluate_lists, "rows": evaluate_rows, "tables": evaluate_tables}
    result = evaluators[kind](nodes, profile)
    current = source_path.read_bytes()
    result.update(
        fixture=source_path.name,
        source_unchanged=current == original,
        source_sha256=hashlib.sha256(original).hexdigest(),
        nodes=nodes,
    )
    result["passes"] = result["passes"] and result["source_unchanged"]
    capture = Path(work) / f"adaptive-{kind}-{profile}"
    capture.mkdir()
    subprocess.run(["weston-screenshooter"], cwd=capture, env=env, check=True, timeout=10)
    screenshot, = capture.glob("*.png")
    shutil.copyfile(screenshot, Path(output))
    return result
