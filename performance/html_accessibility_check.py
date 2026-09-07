"""Native HTML semantics checks; called only inside the isolated capture harness."""
import json
from pathlib import Path
import subprocess
import time


def disclosure_states(nodes, opened):
    expected = [("Closed HTML disclosure", opened), ("Open HTML disclosure", True)]
    if opened:
        expected.append(("Nested HTML disclosure", True))
    elif any(node["name"] == "Nested HTML disclosure" for node in nodes):
        return False
    for name, expanded in expected:
        controls = [node for node in nodes if node["role"] == "button" and node["name"] == name]
        if len(controls) != 1 or "expandable" not in controls[0]["states"]:
            return False
        if ("expanded" in controls[0]["states"]) != expanded:
            return False
    return True


def html_state(nodes, opened):
    names = [node["name"] for node in nodes]
    fragments = [node for node in nodes if node["role"] == "paragraph"
                 and node["name"].startswith("Visible HTML body marker")]
    if len(fragments) != 1:
        return False
    text = fragments[0]["name"]
    visible = ["Visible HTML body marker with strong emphasis and ordinary rich text.",
               "Unicode marker: 東京 café 🦀.", "Closed HTML disclosure",
               "Open HTML disclosure", "Open HTML body marker."]
    if opened:
        visible[3:3] = ["Closed HTML body marker.", "Nested HTML disclosure", "Nested hidden body marker."]
    positions = [text.find(marker) for marker in visible]
    forbidden = ["Display-none body marker.", "Visibility-hidden body marker."]
    if not opened:
        forbidden += ["Closed HTML body marker.", "Nested HTML disclosure", "Nested hidden body marker."]
    summaries = [node for node in nodes if node["role"] == "button"
                 and node["name"] == "Closed HTML disclosure"]
    return (all(text.count(marker) == 1 for marker in visible)
            and positions == sorted(positions)
            and not any(marker in name for marker in forbidden for name in names)
            and any("Unknown original HTML source marker." in name for name in names)
            and any(name == "Following canonical paragraph marker." for name in names)
            and len(summaries) == 1 and "click" in summaries[0]["actions"]
            and disclosure_states(nodes, opened))


def check_html_accessibility(pid, environment, source_path):
    original = source_path.read_bytes()

    def probe(toggle=False):
        command = ["/usr/bin/python3", str(Path(__file__).with_name("accessibility_probe.py")), str(pid)]
        if toggle:
            command.append("--toggle-html")
        result = subprocess.run(command, env=environment, text=True, capture_output=True, timeout=30)
        if result.returncode:
            raise RuntimeError(result.stderr)
        return json.loads(result.stdout)

    initial = probe()
    report = {"initial": initial, "initial_visible_text": html_state(initial["nodes"], False),
              "initial_expansion_state": disclosure_states(initial["nodes"], False)}
    if not report["initial_visible_text"]:
        return dict(report, passes=False)
    for label, opened in [("opened", True), ("restored", False)]:
        activation = probe(toggle=True)
        report[label + "_state_events"] = activation["disclosure_state_events"]
        report[label + "_native_notification"] = any(
            event["source_path"] == activation["disclosure_action"] and event["expanded"] == opened
            for event in activation["disclosure_state_events"])
        deadline = time.monotonic() + 10
        while True:
            state = probe()
            passed = html_state(state["nodes"], opened)
            if passed or time.monotonic() >= deadline:
                break
            time.sleep(0.05)
        report[label] = state
        report[label + "_visible_text"] = passed
        report[label + "_source_unchanged"] = source_path.read_bytes() == original
        if not passed:
            return dict(report, passes=False)
    def fragment_path(snapshot):
        return next(node["path"] for node in snapshot["nodes"]
                    if node["role"] == "paragraph" and node["name"].startswith("Visible HTML body marker"))
    report["fragment_identity_retained"] = len({fragment_path(report[key])
                                               for key in ("initial", "opened", "restored")}) == 1
    report["native_expanded_state_available"] = all(
        disclosure_states(report[key]["nodes"], opened)
        for key, opened in [("initial", False), ("opened", True), ("restored", False)])
    report["disclosure_identity_retained"] = all(
        len({next(node["path"] for node in report[key]["nodes"]
                  if node["role"] == "button" and node["name"] == name)
             for key in ("initial", "opened", "restored")}) == 1
        for name in ("Closed HTML disclosure", "Open HTML disclosure"))
    report["passes"] = all(report[key] for key in (
        "initial_visible_text", "opened_visible_text", "restored_visible_text",
        "opened_source_unchanged", "restored_source_unchanged", "fragment_identity_retained",
        "native_expanded_state_available", "disclosure_identity_retained",
        "opened_native_notification", "restored_native_notification"))
    return report
