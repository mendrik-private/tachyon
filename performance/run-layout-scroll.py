#!/usr/bin/env python3
"""Measure the adaptive redesign's >60 fps scroll contract on the active display.

Uses the existing release instrumentation and kernel uinput client. Run with no
concurrent build/capture workload. Synthetic files and application state are
isolated; the driver scrolls without editing. The original mixed-interaction
qualification remains a separate test.
"""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("qualification", ROOT / "performance/run-release-qualification.py")
qualification = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qualification)


def sample(fixture, output, env, seconds, label):
    state = Path(env["XDG_STATE_HOME"]) / label
    state.mkdir(parents=True, exist_ok=True)
    environment = dict(env, TACHYON_PERF_OUTPUT=str(output), TACHYON_PERF_SECONDS=str(seconds),
                       XDG_STATE_HOME=str(state),
                       TACHYON_PERF_WARMUP_MS="2500", TACHYON_PERF_REFRESH_HZ="120",
                       TACHYON_PERF_LABEL=label, TACHYON_PERF_RESIZE="false",
                       TACHYON_PERF_SCENARIO="continuous bidirectional vertical wheel scrolling",
                       TACHYON_PERF_INPUT_SOURCE="temporary kernel uinput device on active Mutter Wayland display")
    environment.pop("TACHYON_INSTANCE_MODE", None)
    environment.pop("TACHYON_INSTANCE_SOCKET", None)
    load_before = os.getloadavg()
    process = subprocess.Popen([str(qualification.APP), str(fixture)], env=environment,
                               stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
    stream = qualification.start_input_stream()
    try:
        time.sleep(1.5)
        qualification.inject(stream, "move", 6000, 4400)
        qualification.inject(stream, "button", 272, 1)
        qualification.inject(stream, "button", 272, 0)
        stream.stdin.flush()
        deadline = time.monotonic() + seconds + 20
        input_started = time.monotonic()
        tick = 0
        while process.poll() is None and time.monotonic() < deadline:
            # Physical REL_WHEEL is positive upward. First move into the
            # document, then oscillate within it without idling at its top.
            downward = tick < 240 or ((tick - 240) // 160) % 2 == 1
            qualification.inject(stream, "scroll", 0, -1 if downward else 1)
            stream.stdin.flush()
            tick += 1
            time.sleep(0.012)
        if process.poll() is None:
            raise RuntimeError("Scrolling process exceeded its deadline")
        error = process.communicate(timeout=5)[1]
        if process.returncode or not output.exists():
            raise RuntimeError(error[-3000:])
        report = json.loads(output.read_text())
        report["driver"] = {"scroll_events": tick, "duration_seconds": time.monotonic() - input_started,
                            "load_average_before": load_before, "load_average_after": os.getloadavg(),
                            "pattern": "240 ticks down, then repeat 160 up / 160 down; 12 ms target tick"}
        frames = report["presentation"]["interval"]["samples"]
        report["average_presented_fps"] = frames / report["actual_duration_seconds"]
        report["scroll_gate_passes"] = (report["average_presented_fps"] > 60
                                         and report["draw"]["p99_ms"] < 1000 / 60
                                         and report["input"]["latency"]["samples"] > 0)
        return report
    finally:
        if stream.stdin:
            stream.stdin.close()
        stream.wait(timeout=5)
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=10)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--seconds", type=float, default=30)
    parser.add_argument("--runs", type=int, default=2)
    parser.add_argument("--fixture", choices=["100k", "1m", "10m", "adaptive"], action="append")
    parser.add_argument("--output", type=Path, default=ROOT / "performance/adaptive-scroll-2026-09-06.json")
    args = parser.parse_args()
    if args.seconds <= 0 or args.runs < 1:
        parser.error("positive duration and run count required")
    report = {"method": __doc__, "environment": qualification.environment_report(),
              "binary_sha256": hashlib.sha256(qualification.APP.read_bytes()).hexdigest(),
              "fixtures": {}, "reports": []}
    with tempfile.TemporaryDirectory(prefix="tachyon-scroll-") as directory:
        work = Path(directory)
        env = qualification.app_environment(work / "state", work / "cache", work / "shader-cache")
        fixtures = {}
        for label, length in qualification.SIZES:
            fixture = work / f"{label}.md"
            qualification.checked([str(qualification.FIXTURE_GENERATOR), "--bytes", str(length), "--output", str(fixture)])
            fixtures[label] = fixture
        adaptive = "\n\n".join(path.read_text() for path in sorted((ROOT / "performance/layout-fixtures").glob("*.md")))
        adaptive = adaptive.replace("../visual-assets/", str(ROOT / "performance/visual-assets") + "/")
        adaptive = adaptive.replace("(layout-candidates.svg)", "(" + str(ROOT / "performance/layout-fixtures/layout-candidates.svg") + ")")
        fixtures["adaptive"] = work / "adaptive.md"
        fixtures["adaptive"].write_text((adaptive + "\n\n") * 12)
        sample(fixtures["100k"], work / "warmup.json", env, 4, "warmup-not-counted")
        for label, fixture in fixtures.items():
            if args.fixture and label not in args.fixture:
                continue
            contents = fixture.read_bytes()
            report["fixtures"][label] = {"bytes": len(contents), "sha256": hashlib.sha256(contents).hexdigest()}
            for run in range(args.runs):
                result = sample(fixture, work / f"{label}-{run}.json", env, args.seconds, f"{label}-{run + 1}")
                report["reports"].append(result)
                print(f"{result['label']}: {result['average_presented_fps']:.1f} fps, draw p99 {result['draw']['p99_ms']:.2f} ms", flush=True)
        report["passes"] = all(result["scroll_gate_passes"] for result in report["reports"])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(args.output)
    if not report["passes"]:
        raise SystemExit("scroll performance gate failed")


if __name__ == "__main__":
    main()
