#!/usr/bin/env python3
"""Qualify Tachyon against a real Wayland input method in an isolated session.

The harness nests Sway and Fcitx5 inside the repository's private Weston seat.
It never sends input to the user's desktop. A local package extraction can be
supplied with --runtime-prefix when the tools are not installed system-wide.
"""

import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import signal
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parents[1]
HARNESS = ROOT / "performance" / "wayland-harness" / "build"
ORIGINAL = b'''<!-- tachyon-table:v1 {"border":"LogicalPixel","widths":[160,320]} -->
| System IME target marker. | Fixed width |
| --- | --- |
| Candidate popup | Align below the native caret. |
'''
CURSOR_RE = re.compile(
    r"set_cursor_rectangle\((-?\d+), (-?\d+), (-?\d+), (-?\d+)\)"
)


def wait_for(predicate, description, timeout=10.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(0.05)
    raise RuntimeError(f"Timed out waiting for {description}")


def resolve_tool(name, runtime_prefix=None):
    if runtime_prefix:
        candidate = runtime_prefix / "usr" / "bin" / name
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return candidate
    resolved = shutil.which(name)
    if not resolved:
        raise RuntimeError(f"Required tool is unavailable: {name}")
    return Path(resolved)


def semantic_markdown(source):
    """Remove conservative escapes added while serializing a changed text node."""
    escapable = frozenset(b'!"#$%&\'()*+,-./:;<=>?@[\\]^_`{|}~')
    projected = bytearray()
    offset = 0
    while offset < len(source):
        if (source[offset] == ord("\\") and offset + 1 < len(source)
                and source[offset + 1] in escapable):
            projected.append(source[offset + 1])
            offset += 2
        else:
            projected.append(source[offset])
            offset += 1
    return projected.decode("utf-8")


def single_inserted_character(original, edited):
    if len(edited) != len(original) + 1:
        return None
    for offset, character in enumerate(edited):
        if edited[:offset] == original[:offset] and edited[offset + 1:] == original[offset:]:
            return offset, character
    return None


def protocol_excerpt(log):
    excerpt = []
    manager_announced = False
    for line in log.splitlines():
        if "zwp_text_input_v3#" in line:
            excerpt.append(line)
        elif "wl_registry#2." in line and "zwp_text_input_manager_v3" in line:
            if ".bind(" in line or not manager_announced:
                excerpt.append(line)
            manager_announced = manager_announced or ".global(" in line
    return excerpt


def read_log(path):
    return path.read_text(errors="replace") if path.exists() else ""


def log_containing(path, needle):
    log = read_log(path)
    return log if needle in log else None


def last_cursor_rectangle(log):
    matches = CURSOR_RE.findall(log)
    if not matches:
        raise RuntimeError("The client did not publish a text-input cursor rectangle")
    return tuple(int(value) for value in matches[-1])


def candidate_popup_bounds(path):
    try:
        from PIL import Image
    except ImportError as error:
        raise RuntimeError("Pillow is required for candidate-popup geometry") from error
    with Image.open(path) as image:
        pixels = image.convert("RGB")
        points = []
        for y in range(pixels.height):
            for x in range(pixels.width):
                red, green, blue = pixels.getpixel((x, y))
                if red >= 225 and 60 <= green <= 120 and blue <= 30:
                    points.append((x, y))
    if len(points) < 100:
        raise RuntimeError("Fcitx candidate popup accent was not visible")
    return (
        min(x for x, _ in points),
        min(y for _, y in points),
        max(x for x, _ in points),
        max(y for _, y in points),
    )


def send_key(input_client, env, code):
    for state in (1, 0):
        subprocess.run(
            [str(input_client), "key", str(code), str(state)],
            env=env,
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )


def send_undo(input_client, env):
    subprocess.run(
        [str(input_client), "key", "29", "1"], env=env, check=True,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    send_key(input_client, env, 44)
    subprocess.run(
        [str(input_client), "key", "29", "0"], env=env, check=True,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )


def terminate_group(process):
    if process is None or process.poll() is not None:
        return
    os.killpg(process.pid, signal.SIGTERM)
    try:
        process.wait(5)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path,
                        default=ROOT / "target" / "debug" / "tachyon")
    parser.add_argument("--runtime-prefix", type=Path,
                        help="prefix containing locally extracted Sway/Fcitx packages")
    parser.add_argument("--output", type=Path,
                        default=ROOT / "performance" / "layout-previews" / "native-ime-candidate.png")
    parser.add_argument("--protocol-output", type=Path,
                        default=ROOT / "performance" / "layout-previews" / "native-ime-protocol.log")
    parser.add_argument("--report-output", type=Path,
                        default=ROOT / "performance" / "layout-previews" / "native-ime-report.json")
    args = parser.parse_args()

    binary = args.binary.resolve(strict=True)
    runtime_prefix = args.runtime_prefix.resolve(strict=True) if args.runtime_prefix else None
    weston = resolve_tool("weston")
    screenshooter = resolve_tool("weston-screenshooter")
    sway = resolve_tool("sway", runtime_prefix)
    fcitx = resolve_tool("fcitx5", runtime_prefix)
    fcitx_remote = resolve_tool("fcitx5-remote", runtime_prefix)
    dbus_daemon = resolve_tool("dbus-daemon")
    input_client = HARNESS / "input-client"
    input_module = HARNESS / "virtual-input.so"
    if not input_client.is_file() or not input_module.is_file():
        raise RuntimeError("Run performance/wayland-harness/build.sh first")

    for output in (args.output, args.protocol_output, args.report_output):
        output.parent.mkdir(parents=True, exist_ok=True)

    report = {}
    with tempfile.TemporaryDirectory(prefix="tachyon-native-ime-", ignore_cleanup_errors=True) as directory:
        private = Path(directory)
        runtime = private / "runtime"
        runtime.mkdir(mode=0o700)
        config = private / "config"
        (config / "fcitx5").mkdir(parents=True)
        (config / "fcitx5" / "profile").write_text(
            "[Groups/0]\nName=Default\nDefault Layout=us\nDefaultIM=pinyin\n\n"
            "[Groups/0/Items/0]\nName=keyboard-us\nLayout=\n\n"
            "[Groups/0/Items/1]\nName=pinyin\nLayout=\n\n"
            "[GroupOrder]\n0=Default\n",
            encoding="utf-8",
        )
        source = private / "native-ime.md"
        source.write_bytes(ORIGINAL)

        env = dict(
            os.environ,
            XDG_RUNTIME_DIR=str(runtime),
            XDG_STATE_HOME=str(private / "state"),
            XDG_CACHE_HOME=str(private / "cache"),
            XDG_CONFIG_HOME=str(config),
            NO_AT_BRIDGE="1",
        )
        for key in ("DISPLAY", "TACHYON_INSTANCE_MODE", "TACHYON_INSTANCE_SOCKET"):
            env.pop(key, None)
        if runtime_prefix:
            library = runtime_prefix / "usr" / "lib" / "x86_64-linux-gnu"
            env["LD_LIBRARY_PATH"] = ":".join(
                filter(None, (str(library), env.get("LD_LIBRARY_PATH")))
            )
            env["XDG_DATA_DIRS"] = f"{runtime_prefix}/usr/share:/usr/share"
            env["FCITX_ADDON_DIRS"] = str(library / "fcitx5")
            env["FCITX_DATA_DIRS"] = str(runtime_prefix / "usr" / "share" / "fcitx5")
        parent_env = dict(env, WAYLAND_DISPLAY="native-ime-parent")

        weston_process = None
        sway_process = None
        bus_pid = None
        with ExitStack() as files:
            weston_log = files.enter_context((private / "weston.log").open("w"))
            sway_log = files.enter_context((private / "sway.log").open("w"))
            try:
                weston_process = subprocess.Popen(
                    [
                        str(weston), "--backend=headless", "--renderer=gl",
                        "--width=1280", "--height=800", "--socket=native-ime-parent",
                        "--idle-time=0", "--debug", "--no-config", "--shell=kiosk",
                        f"--modules={input_module}",
                    ],
                    env=parent_env, stdout=weston_log, stderr=weston_log,
                    start_new_session=True,
                )
                wait_for(lambda: (runtime / "native-ime-parent").exists(), "private Weston socket")

                bus = subprocess.run(
                    [
                        str(dbus_daemon), "--session", "--fork",
                        f"--address=unix:path={runtime}/bus", "--print-address=1", "--print-pid=1",
                    ],
                    env=env, text=True, capture_output=True, check=True,
                )
                address, pid = bus.stdout.strip().splitlines()
                bus_pid = int(pid)
                session_env = dict(
                    parent_env,
                    DBUS_SESSION_BUS_ADDRESS=address,
                    WLR_BACKENDS="wayland",
                    WLR_RENDERER="pixman",
                    WLR_NO_HARDWARE_CURSORS="1",
                )

                launch = private / "launch.sh"
                launch.write_text(
                    "#!/bin/sh\n"
                    f"env WAYLAND_DEBUG=client {shlex.quote(str(fcitx))} -D "
                    f"--verbose default=4,key_trace=5 >{shlex.quote(str(private / 'fcitx.log'))} 2>&1 &\n"
                    "sleep 2\n"
                    f"env WAYLAND_DEBUG=client {shlex.quote(str(binary))} {shlex.quote(str(source))} "
                    f">{shlex.quote(str(private / 'app.log'))} 2>&1 &\n"
                    "app_pid=$!\n"
                    "sleep 3\n"
                    f"{shlex.quote(str(fcitx_remote))} -s pinyin "
                    f">{shlex.quote(str(private / 'remote.log'))} 2>&1\n"
                    "wait $app_pid\n",
                    encoding="utf-8",
                )
                launch.chmod(0o755)
                sway_config = private / "sway.conf"
                sway_config.write_text(
                    "xwayland disable\ndefault_border none\nfocus_follows_mouse no\n"
                    f"exec {launch}\n"
                    'for_window [app_id="io.github.mendrik_private.Tachyon"] fullscreen enable\n',
                    encoding="utf-8",
                )
                sway_process = subprocess.Popen(
                    [str(sway), "-d", "-c", str(sway_config)],
                    env=session_env, stdout=sway_log, stderr=sway_log,
                    start_new_session=True,
                )
                app_log_path = private / "app.log"
                wait_for(
                    lambda: "zwp_text_input_v3" in read_log(app_log_path),
                    "Tachyon text-input-v3 activation",
                    15,
                )
                time.sleep(3)

                send_key(input_client, parent_env, 49)  # n
                send_key(input_client, parent_env, 23)  # i
                first_preedit = wait_for(
                    lambda: log_containing(app_log_path, 'preedit_string("ni"'),
                    "Fcitx preedit",
                )
                time.sleep(0.7)
                preedit_source_exact = source.read_bytes() == ORIGINAL
                if not preedit_source_exact:
                    raise RuntimeError("Provisional system IME text reached the saved source")

                capture = private / "capture"
                capture.mkdir()
                subprocess.run([str(screenshooter)], cwd=capture, env=parent_env, check=True, timeout=10)
                screenshot, = capture.glob("*.png")
                shutil.copyfile(screenshot, args.output)
                cursor = last_cursor_rectangle(first_preedit)
                popup = candidate_popup_bounds(screenshot)
                expected_popup_origin = (cursor[0], cursor[1] + cursor[3])
                aligned = (
                    abs(popup[0] - expected_popup_origin[0]) <= 2
                    and abs(popup[1] - expected_popup_origin[1]) <= 2
                    and popup[2] - popup[0] >= 100
                    and popup[3] - popup[1] >= 20
                )
                if not aligned:
                    raise RuntimeError(
                        f"Candidate popup {popup} is not aligned below cursor rectangle {cursor}"
                    )

                send_key(input_client, parent_env, 57)  # Space commits the selected candidate.
                wait_for(
                    lambda: source.read_bytes() if source.read_bytes() != ORIGINAL else None,
                    "committed CJK source",
                )
                time.sleep(0.7)
                committed = source.read_bytes()
                insertion = single_inserted_character(
                    ORIGINAL.decode("utf-8"), semantic_markdown(committed)
                )
                if not insertion or ord(insertion[1]) < 128:
                    raise RuntimeError("System IME commit was not exactly one non-ASCII character")
                if b'"widths":[160,320]' not in committed:
                    raise RuntimeError("Committed edit changed the authored table-width metadata")

                send_undo(input_client, parent_env)
                wait_for(lambda: source.read_bytes() == ORIGINAL, "exact IME undo")

                preedit_count = read_log(app_log_path).count('preedit_string("ni"')
                send_key(input_client, parent_env, 49)
                send_key(input_client, parent_env, 23)
                wait_for(
                    lambda: read_log(app_log_path).count('preedit_string("ni"') > preedit_count,
                    "second Fcitx preedit",
                )
                if source.read_bytes() != ORIGINAL:
                    raise RuntimeError("Second provisional composition changed saved source")
                send_key(input_client, parent_env, 1)  # Escape cancels in Fcitx.
                time.sleep(1.5)
                cancelled_source_exact = source.read_bytes() == ORIGINAL
                if not cancelled_source_exact:
                    raise RuntimeError("System IME cancellation rewrote the source")

                final_protocol = read_log(app_log_path)
                excerpt = protocol_excerpt(final_protocol)
                if not any("commit_string" in line for line in excerpt):
                    raise RuntimeError("Missing text-input-v3 commit evidence")
                args.protocol_output.write_text("\n".join(excerpt) + "\n", encoding="utf-8")
                report = {
                    "passes": True,
                    "source_sha256": hashlib.sha256(ORIGINAL).hexdigest(),
                    "preedit_source_exact": preedit_source_exact,
                    "committed_character": insertion[1],
                    "committed_character_offset": insertion[0],
                    "authored_widths_preserved_on_commit": True,
                    "undo_source_exact": True,
                    "cancelled_source_exact": cancelled_source_exact,
                    "cursor_rectangle": cursor,
                    "candidate_popup_bounds": popup,
                    "candidate_popup_expected_origin": expected_popup_origin,
                    "candidate_popup_aligned": aligned,
                    "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                    "protocol": "zwp_text_input_v3 + zwp_input_method_v2",
                    "input_isolation": "Sway and Fcitx5 nested in a private Weston 14 seat",
                }
                args.report_output.write_text(
                    json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
                )
            finally:
                terminate_group(sway_process)
                if bus_pid:
                    try:
                        os.kill(bus_pid, signal.SIGTERM)
                    except ProcessLookupError:
                        pass
                terminate_group(weston_process)

    print(json.dumps(report, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
