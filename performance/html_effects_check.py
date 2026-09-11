"""Native proof that HTML measurement and disclosure state produce no requests."""

from contextlib import contextmanager
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import shutil
import subprocess
import threading
import time
from urllib.request import urlopen


PORT_TOKEN = "TACHYON_INERT_PORT"


class RequestMonitor:
    def __init__(self):
        self._requests = []
        self._lock = threading.Lock()
        self.control_verified = False

    def record(self, method, path):
        with self._lock:
            self._requests.append((method, path))

    def snapshot(self):
        with self._lock:
            return list(self._requests)

    def reset(self):
        with self._lock:
            self._requests.clear()


@contextmanager
def inert_effect_server(source_path):
    """Bind only loopback and inject the ephemeral port into a copied fixture."""
    source_path = Path(source_path)
    source = source_path.read_text()
    if source.count(PORT_TOKEN) < 7:
        raise RuntimeError("Inert HTML fixture lost its bounded request probes")
    monitor = RequestMonitor()

    class Handler(BaseHTTPRequestHandler):
        def _respond(self):
            monitor.record(self.command, self.path)
            self.send_response(204)
            self.end_headers()

        do_GET = _respond
        do_HEAD = _respond
        do_POST = _respond

        def log_message(self, _format, *_args):
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server.daemon_threads = True
    source_path.write_text(source.replace(PORT_TOKEN, str(server.server_port)))
    thread = threading.Thread(target=server.serve_forever, name="inert-html-monitor", daemon=True)
    thread.start()
    try:
        with urlopen(
            f"http://127.0.0.1:{server.server_port}/__monitor_control__", timeout=2
        ) as response:
            if response.status != 204:
                raise RuntimeError("Inert HTML request monitor control probe failed")
        if monitor.snapshot() != [("GET", "/__monitor_control__")]:
            raise RuntimeError("Inert HTML request monitor did not observe its control probe")
        monitor.reset()
        monitor.control_verified = True
        yield monitor
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


def _details(nodes, expanded):
    controls = [node for node in nodes
                if node["role"] == "button" and node["name"] == "Safe authored details"]
    if len(controls) != 1:
        raise RuntimeError(f"Expected one safe disclosure, found {len(controls)}")
    control = controls[0]
    if "expandable" not in control["states"] or "click" not in control["actions"]:
        raise RuntimeError("Safe disclosure lost its native state/action")
    if ("expanded" in control["states"]) != expanded:
        raise RuntimeError(f"Safe disclosure expanded state did not become {expanded}")
    names = [node["name"] for node in nodes]
    body_visible = any("Safe disclosure body marker." in name for name in names)
    if body_visible != expanded:
        raise RuntimeError("Safe disclosure body visibility disagrees with authored view state")
    if not any(name == "Following canonical paragraph remains visible and editable."
               for name in names):
        raise RuntimeError("Content following unsafe HTML disappeared")
    if not any("Unsupported or preserved HTML" in name
               or "Unknown original HTML remains conservatively preserved." in name
               for name in names):
        raise RuntimeError("Unsafe/unknown HTML lost its conservative visible fallback")
    return control


def evaluate(initial, opened, restored, request_snapshots, original, current,
             monitor_control_verified=True):
    initial_control = _details(initial, False)
    opened_control = _details(opened, True)
    restored_control = _details(restored, False)
    checks = {
        "monitor_control_verified": monitor_control_verified,
        "zero_startup_or_measurement_requests": not request_snapshots[0],
        "zero_open_requests": not request_snapshots[1],
        "zero_restore_requests": not request_snapshots[2],
        "disclosure_identity_stable": len({
            initial_control["path"], opened_control["path"], restored_control["path"],
        }) == 1,
        "source_unchanged": current == original,
    }
    return {
        "request_snapshots": request_snapshots,
        "disclosure_path": initial_control["path"],
        "checks": checks,
        "source_sha256": hashlib.sha256(original).hexdigest(),
        "passes": all(checks.values()),
    }


def _capture(env, work, output, stage):
    directory = Path(work) / f"html-effects-{stage}"
    directory.mkdir()
    subprocess.run(["weston-screenshooter"], cwd=directory, env=env, check=True, timeout=10)
    screenshot, = directory.glob("*.png")
    shutil.copyfile(screenshot, Path(output).with_name(f"{Path(output).stem}-{stage}.png"))


def check(env, source_path, pid, output, probe_path, work, monitor):
    source_path = Path(source_path)
    original = source_path.read_bytes()

    def probe(toggle=False):
        command = ["/usr/bin/python3", str(probe_path), str(pid)]
        if toggle:
            command.append("--toggle-html-name=Safe authored details")
        result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=30)
        if result.returncode:
            raise RuntimeError(result.stderr)
        return json.loads(result.stdout)

    time.sleep(0.5)
    initial = probe()["nodes"]
    initial_requests = monitor.snapshot()
    _capture(env, work, output, "initial")

    probe(toggle=True)
    time.sleep(0.5)
    opened = probe()["nodes"]
    opened_requests = monitor.snapshot()
    _capture(env, work, output, "opened")

    probe(toggle=True)
    time.sleep(0.5)
    restored = probe()["nodes"]
    restored_requests = monitor.snapshot()
    _capture(env, work, output, "restored")

    return evaluate(
        initial, opened, restored,
        [initial_requests, opened_requests, restored_requests],
        original, source_path.read_bytes(), monitor.control_verified,
    )
