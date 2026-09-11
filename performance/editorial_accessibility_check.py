"""Structural AT-SPI oracle for the automatic editorial-composition fixture."""


HEADINGS = [
    "Native document workspace",
    "1. Product contract",
    "2. Visual and component system",
    "Typography",
    "Palette",
    "Window anatomy",
    "3. Architecture and persistence",
    "Document core",
    "Document view",
    "Application",
    "4. Explanation and example",
    "5. Final reading section",
]

TASKS = [
    "Rich content inside table cells",
    "Byte-preserving autosave",
    "Files and Outline; no minimap",
    "Automatic layout with stable source order",
    "Light theme and native controls",
]

TABLES = [
    ("Typography table", 5, 3),
    ("Palette table", 5, 2),
    ("Window anatomy table", 4, 2),
]

PEER_CONTENT = [
    ("heading", "Document core"),
    ("paragraph", "Stable nodes, source-preserving import, transactions, selection, undo, and serialization."),
    ("heading", "Document view"),
    ("paragraph", "Measured shaping, automatic arrangements, hit testing, virtualization, tables, HTML, and formulas."),
    ("heading", "Application"),
    ("paragraph", "Native shell, file navigation, autosave, recovery, image loading, and performance instrumentation."),
    ("heading", "4. Explanation and example"),
    ("paragraph", "The explanation belongs with the short example when both fit; on a narrow viewport they stack without changing the document."),
    ("static", "let plan = LayoutPlan::measure(document, viewport);\nassert!(plan.preserves_source_order());\n"),
    ("heading", "5. Final reading section"),
]


def _children(nodes, parent):
    return [node for node in nodes if node["parent"] == parent]


def _one(nodes, role, name):
    matches = [(index, node) for index, node in enumerate(nodes)
               if node["role"] == role and node["name"] == name]
    if len(matches) != 1:
        name_roles = [node["role"] for node in nodes if node["name"] == name]
        raise RuntimeError(
            f"Expected one {role} named {name!r}, found {len(matches)}; name roles={name_roles!r}"
        )
    return matches[0]


def check(nodes):
    headings = [node["name"] for node in nodes if node["role"] == "heading"]
    if headings != HEADINGS:
        raise RuntimeError(f"Editorial heading traversal changed: {headings!r}")

    task_records = [(index, node) for index, node in enumerate(nodes)
                    if node["role"] == "check box"]
    tasks = [node["name"] for _, node in task_records]
    if tasks != TASKS:
        raise RuntimeError(f"Task-list traversal changed: {tasks!r}")
    if any(node["parent"] is None or nodes[node["parent"]]["role"] != "list"
           for _, node in task_records):
        raise RuntimeError("Task checkboxes lost their canonical list parent")

    table_shapes = []
    for table_name, expected_rows, expected_columns in TABLES:
        table_index, _ = _one(nodes, "table", table_name)
        rows = [(index, node) for index, node in enumerate(nodes)
                if node["parent"] == table_index and node["role"] == "table row"]
        if len(rows) != expected_rows:
            raise RuntimeError(f"{table_name} has {len(rows)} rows, expected {expected_rows}")
        widths = []
        for row_ordinal, (row_index, _) in enumerate(rows):
            cells = _children(nodes, row_index)
            expected_role = "column header" if row_ordinal == 0 else "table cell"
            if len(cells) != expected_columns or any(cell["role"] != expected_role for cell in cells):
                raise RuntimeError(f"{table_name} row {row_ordinal + 1} lost its cell hierarchy")
            widths.append(len(cells))
        table_shapes.append(dict(name=table_name, rows=len(rows), columns=widths))

    peer_records = [_one(nodes, "heading", name) for name in
                    ("Document core", "Document view", "Application")]
    peer_bounds = [record[1]["bounds"] for record in peer_records]
    peer_row = (max(bound["y"] for bound in peer_bounds) - min(bound["y"] for bound in peer_bounds) <= 2
                and all(left["x"] + left["width"] <= right["x"]
                        for left, right in zip(peer_bounds, peer_bounds[1:])))
    if not peer_row:
        raise RuntimeError(f"Peer headings are not a source-ordered visual row: {peer_bounds!r}")

    traversal = []
    previous = -1
    for role, name in PEER_CONTENT:
        index, _ = _one(nodes, role, name)
        if index <= previous:
            raise RuntimeError(f"Editorial content is missing, duplicated, or reordered at {name!r}")
        previous = index
        traversal.append(previous)

    _, editor = _one(nodes, "entry", "Markdown document editor")
    editor_focus = "focused" in editor["states"] and "focusable" in editor["states"]
    if not editor_focus:
        raise RuntimeError(f"Document editor did not expose native focus: {editor['states']!r}")

    _one(
        nodes,
        "static",
        "let plan = LayoutPlan::measure(document, viewport);\nassert!(plan.preserves_source_order());\n",
    )
    return {
        "headings": headings,
        "task_items": tasks,
        "task_list_parentage": True,
        "table_shapes": table_shapes,
        "peer_heading_bounds": peer_bounds,
        "peer_row_source_order": peer_row,
        "canonical_content_indices": traversal,
        "editor_focus_exposed": editor_focus,
        "code_text_accessible": True,
        "passes": True,
    }
