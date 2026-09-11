"""Pixel and semantic-state oracle for OS appearance changes."""
import json
import hashlib
import shutil
import subprocess
import time

from theme_portal import set_appearance


PALETTES = {
    "dark": {"page": 0x0f202d, "title_bar": 0x172027, "heading": 0xf2eadc, "text": 0xd7e0e7},
    "light": {"page": 0xfaf9f6, "title_bar": 0x272b2e, "heading": 0x152421, "text": 0x36423e},
}


def evaluate_counts(counts, total, appearance):
    colors = PALETTES[appearance]
    pixels = {role: counts.get(tuple((color >> shift) & 255 for shift in (16, 8, 0)), 0)
              for role, color in colors.items()}
    passes = (pixels["page"] > total * .25 and pixels["title_bar"] > total * .005
              and pixels["heading"] > 50 and pixels["text"] > 50)
    return {"appearance": appearance, "role_pixels": pixels, "passes": passes}


def check_image(path, appearance):
    from PIL import Image
    with Image.open(path) as image:
        total = image.width * image.height
        counts = {color: count for count, color in image.convert("RGB").getcolors(total)}
        # Narrow windows need not contain a sidebar/panel. Validate the actual
        # title-bar region, not matching pixels elsewhere in the document.
        color = PALETTES[appearance]['title_bar']
        title_color = tuple((color >> shift) & 255 for shift in (16, 8, 0))
        title = image.convert('RGB').crop((0, 0, image.width, min(34, image.height)))
        counts[title_color] = dict((color, count) for count, color in title.getcolors(total)).get(title_color, 0)
    return evaluate_counts(counts, total, appearance)


def semantic_geometry(snapshot):
    return [(node["path"], node["role"], node["name"], node.get("bounds"))
            for node in snapshot["nodes"] if node["role"] in
            ("heading", "paragraph", "table", "cell", "entry")]


def editor_state(snapshot):
    editors = [node for node in snapshot["nodes"]
               if node["role"] == "entry" and node["name"] == "Markdown document editor"]
    if len(editors) != 1:
        raise RuntimeError("Expected one accessible document editor")
    editor, = editors
    return {"focused": "focused" in editor["states"]}


def check_cycle(env, pid, work, output, probe_path, source_path, initial,
                input_event, selected=False):
    original = source_path.read_bytes()

    def probe():
        result = subprocess.run(["/usr/bin/python3", str(probe_path), str(pid)],
                                env=env, capture_output=True, text=True, check=True, timeout=30)
        return json.loads(result.stdout)

    def copy_selection():
        if not selected:
            return None
        sentinel = b"__TACHYON_THEME_SELECTION_PROBE__"
        subprocess.run(["wl-copy", "--seat", "tachyon-test", "--type", "text/plain"],
                       input=sentinel, env=env, check=True, timeout=5)
        input_event("key", 29, 1)
        input_event("key", 46, 1)
        input_event("key", 46, 0)
        input_event("key", 29, 0)
        deadline = time.monotonic() + 3
        while True:
            copied = subprocess.run(["wl-paste", "--seat", "tachyon-test", "--no-newline", "--type", "text/plain"],
                                    env=env, check=True, capture_output=True, timeout=5).stdout
            if copied != sentinel or time.monotonic() >= deadline:
                break
            time.sleep(.02)
        if not copied or copied == sentinel:
            raise RuntimeError("Theme selection check requires nonempty native copied text")
        return copied

    before = probe()
    geometry = semantic_geometry(before)
    state = editor_state(before)
    selected_text = copy_selection()
    if not any(node["role"] == "heading" for node in before["nodes"]):
        raise RuntimeError("Theme cycle requires document heading evidence")
    reports = []
    for stage, appearance in enumerate((initial, "light" if initial == "dark" else "dark", initial)):
        set_appearance(env, appearance)
        deadline = time.monotonic() + 8
        attempt = 0
        while True:
            directory = work / f"appearance-{stage}-{attempt}"
            directory.mkdir()
            subprocess.run(["weston-screenshooter"], cwd=directory, env=env, check=True, timeout=10)
            screenshot, = directory.glob("*.png")
            report = check_image(screenshot, appearance)
            if report["passes"] or time.monotonic() >= deadline:
                destination = output.with_name(f"{output.stem}-appearance-{stage}-{appearance}.png")
                shutil.copyfile(screenshot, destination)
                break
            time.sleep(.05)
            attempt += 1
        after = probe()
        report["geometry_and_identity_stable"] = semantic_geometry(after) == geometry
        report["editor_state"] = editor_state(after)
        report["editor_state_stable"] = report["editor_state"] == state
        report["native_selection_copy_stable"] = copy_selection() == selected_text if selected else None
        report["source_unchanged"] = source_path.read_bytes() == original
        report["passes"] &= (report["geometry_and_identity_stable"] and report["source_unchanged"]
                             and report["editor_state_stable"]
                             and report["native_selection_copy_stable"] is not False)
        reports.append(report)
    return {"stages": reports, "passes": all(stage["passes"] for stage in reports),
            "selected_text_sha256": hashlib.sha256(selected_text).hexdigest() if selected else None}
