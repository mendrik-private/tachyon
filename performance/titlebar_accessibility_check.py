"""Check actual native title-bar names/actions, not builder-string presence."""
import argparse
import json
from pathlib import Path


def check(nodes):
    expected = ("Application menu", "Zoom out", "Reset zoom", "Zoom in")
    buttons = [node for node in nodes if node["role"] == "button"]
    named = {name: [node for node in buttons if node["name"] == name] for name in expected}
    names = all(len(matches) == 1 for matches in named.values())
    zoom_actions = all(len(named[name]) == 1 and "click" in named[name][0]["actions"]
                       for name in expected[1:])
    return dict(passes=names and zoom_actions, unique_names=names,
                zoom_actions=zoom_actions, matched={name: len(matches) for name, matches in named.items()})


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tree", type=Path)
    args = parser.parse_args()
    artifact = json.loads(args.tree.read_text())
    result = check(artifact["nodes"])
    result["binary_sha256"] = artifact["binary_sha256"]
    print(json.dumps(result))
    raise SystemExit(0 if result["passes"] else 1)
