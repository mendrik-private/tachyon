import unittest
import string
import copy
import json
from dataclasses import replace
from pathlib import Path

from resize_layout_check import (TECHNICAL, caret_line_visible, committed_viewport_matches, evaluate_editing,
                                evaluate_reading, resize_keys, scenario_for_fixture)


def node(role, name, path, x, y, width=100, height=20, states=None, caret=None):
    record = {
        "role": role,
        "name": name,
        "path": path,
        "bounds": {"x": x, "y": y, "width": width, "height": height},
        "states": states or [],
    }
    if caret is not None:
        record["caret_offset"] = caret
    return record


def snapshot(width, anchor_y, row, caret=42):
    records = [node("entry", "Markdown document editor", "/editor", 0, 64, width, 700,
                    ["focused", "focusable"], caret)]
    for ordinal, name in enumerate(("Native document workspace", "3. Architecture and persistence")):
        records.append(node("heading", name, f"/h/{ordinal}", 20, anchor_y if ordinal else -200))
    for ordinal, name in enumerate(("Document core", "Document view", "Application")):
        x = 20 + ordinal * 150 if row else 20
        y = 160 if row else 160 + ordinal * 90
        records.append(node("heading", name, f"/peer/{ordinal}", x, y, 120))
    return {"nodes": records}


class ResizeLayoutCheckTests(unittest.TestCase):
    def test_continued_record_edit_preserves_every_other_semantic_field(self):
        from resize_layout_check import record_text_transition

        nodes = []

        def add(role, name, parent, bounds=None):
            index = len(nodes)
            nodes.append(dict(role=role, name=name, parent=parent, path=f"/node/{index}",
                              description="", actions=[], bounds=bounds or {}))
            return index

        root = add("document frame", "Document", -1)
        table = add("table", "Package directory", root)
        for row in range(4):
            row_index = add("table row", "", table)
            for col in range(6):
                name = "Repository" if (row, col) == (0, 4) else f"{row}/{col}"
                if (row, col) == (3, 5):
                    name = ""
                bounds = dict(x=20, y=80 + row * 300 + col * 40, width=650, height=24)
                cell = add("column header" if row == 0 else "table cell", name, row_index, bounds)
                add("paragraph", name, cell, bounds)
        while len(nodes) < 88:
            add("paragraph", f"Unchanged {len(nodes)}", root)
        after = copy.deepcopy(nodes)
        indices = [i for i, n in enumerate(nodes) if n["name"] == "Repository"]
        self.assertEqual(len(indices), 2)
        for index in indices:
            after[index]["name"] = "zRepository"
        self.assertTrue(record_text_transition(nodes, after, "Repository", "zRepository"))
        self.assertFalse(record_text_transition(nodes, nodes, "Repository", "zRepository"))
        for index, field, value in ((indices[0], "path", "/replacement"),
                                    (indices[1], "description", "lost semantics"),
                                    (indices[0], "actions", ["new action"]),
                                    (87, "name", "unrelated edit"),
                                    (indices[1], "name", "Repository")):
            broken = copy.deepcopy(after)
            broken[index][field] = value
            self.assertFalse(record_text_transition(nodes, broken, "Repository", "zRepository"))

    def test_resize_extent_waits_for_missing_editor_but_rejects_duplicate_or_corrupt_editor(self):
        from resize_layout_check import editor_extent

        self.assertIsNone(editor_extent({"nodes": [node("frame", "Window", "/window", 0, 0)]}, "width"))
        editor = node("entry", "Markdown document editor", "/editor", 0, 54, 650, 700)
        self.assertEqual(editor_extent({"nodes": [editor]}, "width"), 650)
        with self.assertRaises(RuntimeError):
            editor_extent({"nodes": [editor, editor]}, "width")
        malformed = copy.deepcopy(editor)
        del malformed["bounds"]["width"]
        with self.assertRaises(KeyError):
            editor_extent({"nodes": [malformed]}, "width")

    def test_restored_record_edit_retains_narrow_measure_until_release(self):
        from unittest.mock import patch
        from resize_layout_check import evaluate_editing_restoration

        scenario = scenario_for_fixture("122-paired-records.md", "record-body")

        def sample(width, row=False, measure=650):
            return {"nodes": [
                node("entry", "Markdown document editor", "/editor", 0, 54, width, 700, ["focused"]),
                node("heading", scenario.anchor, "/heading", 20, 76),
                *(node("table cell", name, f"/record/{i}", 680 * i if row else 0,
                       150 if row else 150 + 300 * i, measure, 24)
                  for i, name in enumerate(scenario.peers)),
            ]}

        state = dict(selection_start=42, selection_end=42, focused=True,
                     viewport_bounds=[0, 54, 1380, 754], caret_bounds=[400, 200, 402, 224])
        # Isolate the restoration contract; complete table ownership is checked
        # by record_geometry tests and the native 88-node snapshots.
        with patch("resize_layout_check._record_checks", return_value={}):
            for restored, expected in ((sample(1380), True), (sample(1380, True), False),
                                       (sample(1380, measure=600), False)):
                report = evaluate_editing_restoration(sample(1380, True), restored,
                    state, state, b"exact edit", b"exact edit", scenario, frozen=sample(650))
                self.assertEqual(report["passes"], expected, report)

    def test_record_targets_follow_canvas_pressure_and_exact_cell_source(self):
        source = (Path(__file__).parent / "layout-fixtures/122-paired-records.md").read_text()
        for target in ("record-body", "record-header"):
            for zoom in (1, 1.5, 2):
                scenario = scenario_for_fixture("122-paired-records.md", target, zoom)
                self.assertTrue(scenario.paired_records)
                self.assertFalse(scenario.escape_edit)
                self.assertEqual(scenario.wide_row, zoom == 1)
                self.assertEqual(scenario.height_stacks, zoom != 1)
                self.assertEqual(source.count(scenario.edit_paragraph), 1)
                self.assertEqual(scenario.peer_roles, ("table cell", "table cell"))
        for fixture, target, zoom in (("76-entity-records.md", "record-body", 1),
                                      ("122-paired-records.md", "record-body", 1.2),
                                      ("122-paired-records.md", "record-unknown", 1)):
            with self.assertRaises(ValueError):
                scenario_for_fixture(fixture, target, zoom)

    def test_record_edit_golden_rejects_changes_to_other_fields(self):
        source = (Path(__file__).parent / "layout-fixtures/122-paired-records.md").read_bytes()
        for target in ("record-body", "record-header"):
            # Isolate the source oracle. Native runs additionally check every
            # canonical node, record field and the full caret rectangle.
            scenario = replace(scenario_for_fixture("122-paired-records.md", target), paired_records=False)
            saved = source.replace(scenario.edit_paragraph.encode(),
                                   ("x" + scenario.edit_paragraph).encode(), 1)

            def sample(width, row):
                records = [node("entry", "Markdown document editor", "/editor", 0, 64, width, 700, ["focused"]),
                           node("heading", scenario.anchor, "/heading", 20, 76)]
                records.extend(node("table cell", name, f"/record/{i}",
                                    20 + 220 * i if row else 20, 150 if row else 150 + 300 * i, 200)
                               for i, name in enumerate(scenario.peers))
                return {"nodes": records}

            state = dict(selection_start=2, selection_end=2, focused=True)
            good = evaluate_editing(sample(1380, True), sample(650, False), state, state,
                                    source, saved, source, scenario)
            self.assertTrue(good["passes"], good)
            for wrong in (saved + b"\n", saved.replace(b"48", b"96")):
                bad = evaluate_editing(sample(1380, True), sample(650, False), state, state,
                                       source, wrong, source, scenario)
                self.assertFalse(bad["checks"]["edited_source_exact"])

    def test_live_record_reader_waits_for_complete_line_and_rejects_corruption(self):
        from resize_layout_check import prefixed_records

        for prefix in ("TACHYON_RESIZE_STATE ", "TACHYON_LAYOUT_TRACE "):
            complete = (prefix + '{"sequence":1}\n').encode()
            partial = (prefix + '{"sequence":').encode()
            self.assertEqual(prefixed_records(complete + partial, prefix), [{"sequence": 1}])
            self.assertEqual(prefixed_records(complete + partial + b'2}\n', prefix),
                             [{"sequence": 1}, {"sequence": 2}])
            self.assertEqual(prefixed_records((prefix + '{"sequence":1}').encode(), prefix), [])
            with self.assertRaises(json.JSONDecodeError):
                prefixed_records(complete + (prefix + '{BROKEN}\n').encode(), prefix)
            self.assertEqual(prefixed_records(b"unrelated stderr\n" + complete, prefix), [{"sequence": 1}])
            partial_utf8 = (prefix + '{"name":"東京').encode()[:-1]
            self.assertEqual(prefixed_records(complete + partial_utf8, prefix), [{"sequence": 1}])
            unicode_record = (prefix + '{"name":"東京\u2028retained"}\n').encode()
            self.assertEqual(prefixed_records(unicode_record, prefix), [{"name": "東京\u2028retained"}])

    def test_edit_restoration_rejects_invisible_caret_wrong_width_and_changed_source(self):
        from resize_layout_check import caret_visible, evaluate_editing_restoration

        scenario = replace(scenario_for_fixture("119-rich-timelines.md", "timeline-body"), rich_timeline=False)
        def sample(width, row=False):
            return {"nodes": [
                node("entry", "Markdown document editor", "/editor", 0, 54, width, 700, ["focused"]),
                node("heading", scenario.anchor, "/heading", 20, 76),
                *(node("paragraph", date, f"/date/{i}", 20 + 220 * i if row else 20,
                       150 if row else 150 + 300 * i, 200)
                  for i, date in enumerate(scenario.peers)),
            ]}
        state = dict(selection_start=42, selection_end=42, focused=True,
                     viewport_bounds=[0, 54, 1380, 754], caret_bounds=[400, 200, 402, 224])
        source = b"Exactly one x in the authored target.\n"
        good = evaluate_editing_restoration(sample(1380), sample(1380), state, state, source, source, scenario)
        self.assertTrue(good["passes"], good)
        for snapshot, restored_state, restored_source in (
                (sample(650), state, source), (sample(1380, True), state, source),
                (sample(1380), dict(state, caret_bounds=[1500, 200, 1502, 224]), source),
                (sample(1380), dict(state, selection_start=43, selection_end=43), source),
                (sample(1380), state, source + b"\n")):
            bad = evaluate_editing_restoration(sample(1380), snapshot, state, restored_state, source, restored_source, scenario)
            self.assertFalse(bad["passes"], bad)
        self.assertFalse(caret_visible(dict(state, caret_bounds=[-10, 200, -8, 224])))

    def test_caret_visibility_requires_native_viewport_not_only_stable_offsets(self):
        state = dict(selection_start=42, selection_end=42, focused=True,
                     viewport_bounds=[20, 54, 1000, 354], caret_bounds=[50, 300, 52, 324])
        self.assertTrue(caret_line_visible(state))
        for caret in (None, [50, 514, 52, 538], [50, 40, 52, 64], [50, 54, 52, float("nan")]):
            self.assertFalse(caret_line_visible(dict(state, caret_bounds=caret)))
        self.assertFalse(caret_line_visible(dict(state, viewport_bounds=None)))

    def test_rich_event_support_oracle_checks_nested_leaves_and_next_date_boundary(self):
        from rich_timeline_check import DATES, event_support_geometry

        nodes = []

        def append(role, name, parent, y, height=20):
            index = len(nodes)
            record = node(role, name, f"/{index}", 20, y, height=height)
            record["parent"] = parent
            nodes.append(record)
            return index

        root = append("list", "List", -1, 0)
        for index, date in enumerate(DATES):
            y = 100 + 200 * index
            item = append("list item", date + " Event and support", root, y)
            append("paragraph", date + " Event", item, y)
            append("paragraph", "Supporting body", item, y + 30)
            nested = append("list", "Nested children", item, y + 70)
            child = append("list item", "Child", nested, y + 70)
            append("paragraph", "Nested supporting text", child, y + 70)
        append("heading", "Ordinary requirements", -1, 700)
        good = event_support_geometry(nodes)
        self.assertTrue(good["passes"], good)
        self.assertEqual([event["support_count"] for event in good["events"]], [2, 2, 2])
        for y in (90, 295):
            bad = copy.deepcopy(nodes)
            bad[6]["bounds"]["y"] = y
            self.assertFalse(event_support_geometry(bad)["passes"])
        bad = copy.deepcopy(nodes)
        bad[6]["bounds"]["height"] = 0
        self.assertFalse(event_support_geometry(bad)["passes"])

    def test_rich_timeline_exact_edit_oracle_preserves_indentation_and_literal_code(self):
        source = (Path(__file__).parent / "layout-fixtures/119-rich-timelines.md").read_bytes()
        for target in ("timeline-body", "timeline-code", "timeline-table", "timeline-nested"):
            # This test isolates exact source and date topology. Native runs
            # additionally require the complete 68-node canonical document.
            scenario = replace(scenario_for_fixture("119-rich-timelines.md", target), rich_timeline=False)
            value = "x" + scenario.edit_paragraph
            escaped = "".join("\\" + char if char in string.punctuation and char != "|"
                              else char for char in value)
            saved = source.replace(scenario.edit_paragraph.encode(),
                                   (escaped if scenario.escape_edit else value).encode(), 1)

            def sample(width, row=False):
                records = [node("entry", "Markdown document editor", "/editor", 0, 64, width, 700, ["focused"]),
                           node("heading", scenario.anchor, "/heading", 20, 76)]
                records.extend(node("paragraph", date, f"/date/{i}",
                                    20 + 220 * i if row else 20, 150 if row else 150 + 300 * i, 200)
                               for i, date in enumerate(scenario.peers))
                return {"nodes": records}

            state = dict(selection_start=2, selection_end=2, focused=True)
            good = evaluate_editing(sample(1380), sample(650), state, state, source, saved, source, scenario)
            self.assertTrue(good["passes"], good)
            for wrong in (saved + b"\n", saved.replace(b"  ```sh", b"```sh", 1)):
                bad = evaluate_editing(sample(1380), sample(650), state, state, source, wrong, source, scenario)
                self.assertFalse(bad["checks"]["edited_source_exact"])
            bad = evaluate_editing(sample(1380, True), sample(650), state, state, source, saved, source, scenario)
            self.assertFalse(bad["checks"]["wide_peer_stack"])

    def test_rich_timeline_targets_keep_vertical_events_and_exact_source_targets(self):
        source = (Path(__file__).parent / "layout-fixtures/119-rich-timelines.md").read_text()
        for target in ("timeline-body", "timeline-code", "timeline-table", "timeline-nested"):
            for zoom in (1, 1.5, 2):
                scenario = scenario_for_fixture("119-rich-timelines.md", target, zoom)
                self.assertFalse(scenario.wide_row)
                self.assertTrue(scenario.height_stacks)
                self.assertTrue(scenario.rich_timeline)
                self.assertEqual(scenario.peer_roles, ("paragraph",) * 3)
                self.assertEqual(source.count(scenario.edit_paragraph), 1)
                self.assertEqual(scenario.escape_edit, target in ("timeline-body", "timeline-nested"))
                self.assertEqual(scenario.reading_setup_tail, "Compact milestones")
        for fixture, target, zoom in (("01-field-notes.md", "timeline-body", 1),
                                      ("119-rich-timelines.md", "timeline-code", 1.2),
                                      ("119-rich-timelines.md", "timeline-unknown", 1)):
            with self.assertRaises(ValueError):
                scenario_for_fixture(fixture, target, zoom)

    def test_specification_resize_targets_preserve_exact_code_and_zoom_fit_contract(self):
        source = (Path(__file__).parent / "layout-fixtures/03-technical-reference.md").read_bytes()
        for target in ("specification-table", "specification-code"):
            for zoom in (1, 1.5, 2):
                scenario = scenario_for_fixture("03-technical-reference.md", target, zoom)
                self.assertEqual(scenario.wide_row, zoom == 1)
                self.assertEqual(scenario.peer_roles, ("table", "static"))
                self.assertFalse(scenario.escape_edit)
                self.assertEqual(source.count(scenario.edit_paragraph.encode()), 1)
        with self.assertRaises(ValueError):
            scenario_for_fixture("01-field-notes.md", "specification-code")
        with self.assertRaises(ValueError):
            scenario_for_fixture("03-technical-reference.md", "specification-code", 1.2)

    def test_specification_edit_and_reading_oracles_reject_wrong_topology_and_source(self):
        source = (Path(__file__).parent / "layout-fixtures/03-technical-reference.md").read_bytes()
        for target in ("specification-table", "specification-code"):
            for zoom in (1, 1.5, 2):
                scenario = scenario_for_fixture("03-technical-reference.md", target, zoom)
                value = "x" + scenario.edit_paragraph
                escaped = "".join("\\" + char if char in string.punctuation and char != "|"
                                  else char for char in value)
                saved = source.replace(scenario.edit_paragraph.encode(),
                                       (escaped if scenario.escape_edit else value).encode(), 1)

                def sample(width, paired, editing=False):
                    code = value if editing and target == "specification-code" else scenario.peers[1]
                    return {"nodes": [
                        node("entry", "Markdown document editor", "/editor", 0, 64, width, 700, ["focused"]),
                        node("heading", scenario.anchor, "/heading", 20, 76),
                        node("table", scenario.peers[0], "/table", 20, 150, 400, 185),
                        node("static", code, "/code", 444 if paired else 20,
                             168 if paired else 450, 400, 126),
                    ]}

                wide, narrow = sample(1380, scenario.wide_row), sample(650, False)
                good = evaluate_reading(wide, narrow, narrow, wide, scenario)
                self.assertTrue(good["passes"], good)
                bad = evaluate_reading(wide, narrow, narrow, sample(1380, not scenario.wide_row), scenario)
                self.assertFalse(bad["passes"])
                state = dict(selection_start=2, selection_end=2, focused=True)
                before, after = sample(1380, scenario.wide_row, True), sample(650, False, True)
                good = evaluate_editing(before, after, state, state, source, saved, source, scenario)
                self.assertTrue(good["passes"], good)
                for corrupted in (saved + b"\n", saved.replace(b"## Authentication", b"## Changed", 1)):
                    bad = evaluate_editing(before, after, state, state, source, corrupted, source, scenario)
                    self.assertFalse(bad["checks"]["edited_source_exact"])
                if target == "specification-code":
                    # Code punctuation and escapes are exact source, not prose
                    # serialization that the oracle may normalize away.
                    incorrectly_escaped = source.replace(scenario.edit_paragraph.encode(), escaped.encode(), 1)
                    with self.assertRaises(RuntimeError):
                        evaluate_editing(before, after, state, state, source, incorrectly_escaped, source, scenario)
                else:
                    incorrectly_escaped = source.replace(scenario.edit_paragraph.encode(), escaped.encode(), 1)
                    bad = evaluate_editing(before, after, state, state, source, incorrectly_escaped, source, scenario)
                    self.assertFalse(bad["checks"]["edited_source_exact"])

    def test_short_guidance_requires_retained_row_for_reading_and_editing(self):
        scenario = scenario_for_fixture("01-field-notes.md")
        source = (Path(__file__).parent / "layout-fixtures/01-field-notes.md").read_bytes()
        edited_text = "x" + scenario.edit_paragraph
        escaped = "".join("\\" + char if char in string.punctuation and char != "|"
                          else char for char in edited_text).encode()
        edited = source.replace(scenario.edit_paragraph.encode(), escaped, 1)

        def sample(height, paired, editing=False):
            peers = (edited_text, scenario.peers[1]) if editing else scenario.peers
            return {"nodes": [
                node("entry", "Markdown document editor", "/editor", 0, 64, 1390, height, ["focused"]),
                node("heading", scenario.anchor, "/heading", 20, 64),
                node("notification", peers[0], "/note", 20, 120, 640, 48),
                node("notification", peers[1], "/tip", 684 if paired else 20,
                     120 if paired else 220, 640, 48),
            ]}

        tall, short, wrong = sample(884, True), sample(304, True), sample(304, False)
        self.assertTrue(evaluate_reading(tall, short, short, tall, scenario, axis="height")["passes"])
        self.assertFalse(evaluate_reading(tall, wrong, wrong, tall, scenario, axis="height")["checks"]["short_peer_row"])
        state = dict(selection_start=2, selection_end=2, focused=True)
        for paired in (True, False):
            result = evaluate_editing(sample(884, True, True), sample(304, paired, True),
                                      state, state, source, edited, source, scenario, axis="height")
            self.assertEqual(result["passes"], paired, result)
            self.assertEqual(result["checks"]["short_peer_row"], paired)
        initially_stacked = evaluate_editing(sample(884, False, True), sample(304, True, True),
                                             state, state, source, edited, source, scenario, axis="height")
        self.assertFalse(initially_stacked["passes"])
        self.assertFalse(initially_stacked["checks"]["wide_peer_row"])

    def test_recent_pair_scenarios_use_sufficient_width_and_distinct_height_contracts(self):
        guidance = scenario_for_fixture("01-field-notes.md")
        tables = scenario_for_fixture("03-technical-reference.md")
        self.assertEqual((guidance.wide_key, tables.wide_key), (63, 63))
        self.assertFalse(guidance.height_stacks)
        self.assertTrue(tables.height_stacks)
        self.assertEqual(guidance.peer_roles, ("notification", "notification"))
        self.assertEqual(tables.peers, ("Configuration", "Comparison across environments"))
        for fixture, scenario in (("01-field-notes.md", guidance), ("03-technical-reference.md", tables)):
            source = (Path(__file__).parent / "layout-fixtures" / fixture).read_text()
            self.assertEqual(source.count(scenario.edit_paragraph), 1)
        self.assertEqual(resize_keys(67, True, wide_key=63), [67, 63] * 4 + [67])
        self.assertEqual(resize_keys(63, True, wide_key=63), [63, 67] * 4 + [63])
        with self.assertRaises(ValueError):
            resize_keys(68, True, wide_key=63)

    def test_content_first_soft_break_edit_preserves_all_other_source_bytes(self):
        for fixture in ("116-code-led-explanations.md", "117-table-led-resize.md", "118-opening-overview.md"):
            scenario = scenario_for_fixture(fixture)
            source = (Path(__file__).parent / "layout-fixtures" / fixture).read_bytes()
            self.assertIn("\n", scenario.edit_source)
            self.assertNotIn("\n", scenario.edit_paragraph)
            self.assertEqual(scenario.peer_roles[1], "paragraph")
            value = "x" + scenario.edit_paragraph
            escaped = "".join("\\" + character if character in string.punctuation and character != "|"
                              else character for character in value).encode()
            edited = source.replace(scenario.edit_source.encode(), escaped, 1)

            def sample(width, paired):
                return {"nodes": [
                    node("entry", "Markdown document editor", "/editor", 0, 64, width, 700, ["focused"]),
                    node("heading", scenario.anchor, "/heading", 20, 64),
                    node(scenario.peer_roles[0], scenario.peers[0], "/technical", 20, 168, 400, 231),
                    node("paragraph", value, "/prose", 444 if paired else 20,
                         120 if paired else 420, 400, 220),
                ]}

            state = dict(selection_start=2, selection_end=2, focused=True)
            before, after = sample(1100, True), sample(680, False)
            good = evaluate_editing(before, after, state, state, source, edited, source, scenario)
            self.assertTrue(good["passes"], good)
            for bad in (edited + b"\n", edited.replace(b"## ", b"## Changed ", 1)):
                result = evaluate_editing(before, after, state, state, source, bad, source, scenario)
                self.assertFalse(result["checks"]["edited_source_exact"])

    def test_mixed_resize_edit_rejects_unrelated_saved_byte_changes(self):
        scenario = scenario_for_fixture("107-extended-code.md")
        source = (Path(__file__).parent / "layout-fixtures/107-extended-code.md").read_bytes()
        value = "x" + scenario.edit_paragraph
        # The plain fixture has only commas and periods to escape.
        edited = source.replace(scenario.edit_paragraph.encode(), value.replace(",", "\\,").replace(".", "\\.").encode())
        def sample(width, text, paired):
            return {"nodes": [
                node("entry", "Markdown document editor", "/editor", 0, 64, width, 700, ["focused"]),
                node("heading", scenario.anchor, "/heading", 20, 64),
                node("paragraph", text, "/prose", 20, 120, 400, 220),
                node("static", scenario.peers[1], "/code", 444 if paired else 20,
                     168 if paired else 400, 400, 231),
            ]}
        state = dict(selection_start=2, selection_end=2, focused=True)
        before, after = sample(1100, value, True), sample(680, value, False)
        good = evaluate_editing(before, after, state, state, source, edited, source, scenario)
        self.assertTrue(good["passes"])
        bad = evaluate_editing(before, after, state, state, source, edited + b"\n", source, scenario)
        self.assertFalse(bad["passes"])
        self.assertFalse(bad["checks"]["edited_source_exact"])

    def test_explanation_recovery_requires_real_disjoint_semantic_peers(self):
        for fixture in ("106-extended-table.md", "107-extended-code.md"):
            scenario = scenario_for_fixture(fixture)

            def sample(width, paired):
                return {"nodes": [
                    node("entry", "Markdown document editor", "/editor", 0, 64, width, 700),
                    node("heading", scenario.anchor, "/anchor", 20, 64),
                    node(scenario.peer_roles[0], scenario.peers[0], "/prose", 20, 120, 400, 220),
                    node(scenario.peer_roles[1], scenario.peers[1], "/technical",
                         444 if paired else 20, 168 if paired else 400, 400, 231),
                ]}

            wide, narrow = sample(1100, True), sample(680, False)
            self.assertTrue(evaluate_reading(wide, narrow, narrow, wide, scenario)["passes"])
            for field, value in (("x", 400), ("y", 400)):
                bad = sample(1100, True)
                bad["nodes"][-1]["bounds"][field] = value
                self.assertFalse(evaluate_reading(wide, narrow, narrow, bad, scenario)["passes"])
            changed = sample(1100, True)
            changed["nodes"][-1]["path"] = "/replacement"
            self.assertFalse(evaluate_reading(wide, narrow, narrow, changed, scenario)["passes"])
            missing = sample(1100, True)
            missing["nodes"].pop()
            with self.assertRaises(RuntimeError):
                evaluate_reading(wide, narrow, narrow, missing, scenario)

    def test_burst_alternates_and_finishes_at_requested_target(self):
        for target in (67, 68):
            self.assertEqual(resize_keys(target, False), [target])
            keys = resize_keys(target, True)
            self.assertEqual(len(keys), 9)
            self.assertEqual(keys[-1], target)
            self.assertTrue(all(a != b for a, b in zip(keys, keys[1:])))
        with self.assertRaises(ValueError):
            resize_keys(69, True)

    def test_resize_requires_latest_committed_geometry_at_actual_viewport(self):
        current = snapshot(1100, 64, True)
        good = dict(committed=True, canvas_width=1100, viewport_height=700,
                    text_zoom=1, planner_failed=False)
        self.assertTrue(committed_viewport_matches([good], current))
        self.assertTrue(committed_viewport_matches([good], current, expected_zoom=1))
        self.assertFalse(committed_viewport_matches([good], current, expected_zoom=1.5))
        self.assertFalse(committed_viewport_matches([], current))
        for invalid in (dict(good, canvas_width=680), dict(good, viewport_height=300),
                        dict(good, planner_failed=True), dict(good, text_zoom=0)):
            self.assertFalse(committed_viewport_matches([good, invalid], current))
        self.assertFalse(committed_viewport_matches([dict(good, committed=False)], current))
        self.assertTrue(committed_viewport_matches(
            [good, dict(good, committed=False, canvas_width=680)], current))
        self.assertTrue(committed_viewport_matches(
            [dict(good, canvas_width=550, viewport_height=350, text_zoom=2)], current))

    def test_height_resize_requires_changed_height_and_unchanged_width(self):
        tall = snapshot(1100, 64, True)
        short = snapshot(1100, 64, False)
        short["nodes"][0]["bounds"]["height"] = 300
        self.assertTrue(evaluate_reading(tall, short, short, tall, axis="height")["passes"])
        short["nodes"][0]["bounds"]["width"] = 1000
        result = evaluate_reading(tall, short, short, tall, axis="height")
        self.assertFalse(result["passes"])
        self.assertFalse(result["checks"]["width_unchanged"])
        unchanged = snapshot(1100, 64, False)
        self.assertFalse(evaluate_reading(tall, unchanged, unchanged, tall, axis="height")["passes"])

    def test_reading_rejects_drift_beyond_one_physical_pixel(self):
        for delta, scale in [(2, 120), (1, 150), (1, 240)]:
            with self.subTest(delta=delta, scale=scale):
                result = evaluate_reading(
                    snapshot(1100, 64, True), snapshot(680, 64 + delta, False),
                    snapshot(680, 64 + delta, False), snapshot(1100, 64, True), scale=scale,
                )
                self.assertFalse(result["passes"])
                self.assertFalse(result["checks"]["reading_anchor_stable"])

    def test_technical_pair_requires_both_peers_and_source_order(self):
        def technical_snapshot(width, row):
            record = snapshot(width, 64, row)
            record["nodes"] = record["nodes"][:1] + record["nodes"][3:5]
            for entry, title in zip(record["nodes"][1:], TECHNICAL.peers):
                entry["name"] = title
            return record
        wide = technical_snapshot(1100, True)
        narrow = technical_snapshot(680, False)
        self.assertTrue(evaluate_reading(wide, narrow, narrow, wide, TECHNICAL)["passes"])
        narrow["nodes"][2]["bounds"]["y"] = 120
        self.assertFalse(evaluate_reading(wide, narrow, narrow, wide, TECHNICAL)["passes"])
        narrow["nodes"].pop()
        with self.assertRaises(RuntimeError):
            evaluate_reading(wide, narrow, narrow, wide, TECHNICAL)

    def test_scenario_requires_an_explicit_fixture(self):
        self.assertEqual(scenario_for_fixture("79-technical-sections.md"), TECHNICAL)
        with self.assertRaises(ValueError):
            scenario_for_fixture("other.md")

    def test_reading_requires_stable_anchor_identity_and_legal_layouts(self):
        result = evaluate_reading(
            snapshot(1100, 64, True), snapshot(680, 65, False),
            snapshot(680, 65, False), snapshot(1100, 64, True),
        )
        self.assertTrue(result["passes"])

    def test_reading_rejects_anchor_jump(self):
        result = evaluate_reading(
            snapshot(1100, 64, True), snapshot(680, 90, False),
            snapshot(680, 90, False), snapshot(1100, 64, True),
        )
        self.assertFalse(result["checks"]["reading_anchor_stable"])
        self.assertFalse(result["passes"])

    def test_editing_requires_caret_source_and_undo_stability(self):
        original = ("prefix Measured shaping, automatic arrangements, hit testing, virtualization, "
                    "tables, HTML, and formulas. suffix").encode()
        offset = original.decode().index("formulas")
        edited = original[:offset] + b"x" + original[offset:]
        result = evaluate_editing(
            snapshot(1100, 64, True, 73), snapshot(680, 64, False, 73),
            {"selection_start": 73, "selection_end": 73, "focused": True},
            {"selection_start": 73, "selection_end": 73, "focused": True},
            original, edited, original,
        )
        self.assertTrue(result["passes"])

    def test_editing_rejects_caret_loss(self):
        original = ("prefix Measured shaping, automatic arrangements, hit testing, virtualization, "
                    "tables, HTML, and formulas. suffix").encode()
        offset = original.decode().index("formulas")
        edited = original[:offset] + b"x" + original[offset:]
        result = evaluate_editing(
            snapshot(1100, 64, True, 73), snapshot(680, 64, False, 0),
            {"selection_start": 73, "selection_end": 73, "focused": True},
            {"selection_start": 0, "selection_end": 0, "focused": True},
            original, edited, original,
        )
        self.assertFalse(result["checks"]["caret_stable"])
        self.assertFalse(result["passes"])


if __name__ == "__main__":
    unittest.main()
