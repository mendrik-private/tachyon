#!/usr/bin/env python3
"""Run Mineral's process-start and compositor performance qualification."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parents[1]
PERFORMANCE = ROOT / "performance"
APP = ROOT / "target/release/mineral-markdown"
FIXTURE_GENERATOR = ROOT / "target/release/mineral-fixture"
UINPUT_CLIENT = PERFORMANCE / "wayland-harness/build/uinput-client"
SIZES = (("100k", 100 * 1024), ("1m", 1024 * 1024), ("10m", 10 * 1024 * 1024))


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--startup-runs", type=int, default=30)
    parser.add_argument("--interaction-runs", type=int, default=5)
    parser.add_argument("--seconds", type=float, default=60.0)
    parser.add_argument("--warmup-ms", type=int, default=2500)
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--skip-startup", action="store_true")
    parser.add_argument("--skip-interaction", action="store_true")
    parser.add_argument(
        "--output",
        type=Path,
        default=PERFORMANCE
        / f"release-qualification-{dt.date.today().isoformat()}.json",
    )
    args = parser.parse_args()
    if args.startup_runs < 1 or args.interaction_runs < 1:
        parser.error("run counts must be positive")
    if args.seconds <= 0 or args.warmup_ms < 0:
        parser.error("duration must be positive and warmup nonnegative")
    if args.skip_startup and args.skip_interaction:
        parser.error("cannot skip both measurement phases")
    return args


def checked(command: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, check=True, text=True, **kwargs)


def build() -> None:
    print("Building optimized binaries and Wayland harness…", flush=True)
    checked(["cargo", "build", "--release", "-p", "markdown-app", "--bins"], cwd=ROOT)
    checked([str(PERFORMANCE / "wayland-harness/build.sh")], cwd=ROOT)


def generate_fixtures() -> dict[str, Path]:
    generated = PERFORMANCE / "generated"
    generated.mkdir(parents=True, exist_ok=True)
    fixtures: dict[str, Path] = {}
    for label, size in SIZES:
        path = generated / f"qualification-{label}.md"
        checked(
            [
                str(FIXTURE_GENERATOR),
                "--bytes",
                str(size),
                "--output",
                str(path),
            ],
            cwd=ROOT,
        )
        fixtures[label] = path
    return fixtures


def app_environment(state: Path, cache: Path, shader_cache: Path) -> dict[str, str]:
    environment = os.environ.copy()
    for directory in (state, cache, shader_cache):
        directory.mkdir(parents=True, exist_ok=True)
    environment.update(
        {
            "XDG_STATE_HOME": str(state),
            "XDG_CACHE_HOME": str(cache),
            "MESA_SHADER_CACHE_DIR": str(shader_cache),
        }
    )
    return environment


def evict_file(path: Path) -> None:
    if not hasattr(os, "posix_fadvise"):
        raise RuntimeError("Python lacks posix_fadvise; cold-file qualification is unavailable")
    descriptor = os.open(path, os.O_RDONLY)
    try:
        os.posix_fadvise(descriptor, 0, 0, os.POSIX_FADV_DONTNEED)
    finally:
        os.close(descriptor)


def startup_sample(
    fixture: Path,
    output: Path,
    label: str,
    cache_state: str,
    environment: dict[str, str],
) -> dict[str, object]:
    output.parent.mkdir(parents=True, exist_ok=True)
    output.unlink(missing_ok=True)
    run_environment = environment.copy()
    run_environment.update(
        {
            "MINERAL_STARTUP_OUTPUT": str(output),
            "MINERAL_STARTUP_LABEL": label,
            "MINERAL_STARTUP_CACHE_STATE": cache_state,
        }
    )
    result = subprocess.run(
        [str(APP), str(fixture)],
        cwd=ROOT,
        env=run_environment,
        text=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        timeout=60,
    )
    if result.returncode != 0 or not output.exists():
        raise RuntimeError(
            f"startup sample {label} failed ({result.returncode}): {result.stderr[-2000:]}"
        )
    return json.loads(output.read_text())


def start_resident_server(
    fixture: Path,
    output: Path,
    socket: Path,
    environment: dict[str, str],
) -> tuple[subprocess.Popen[str], dict[str, object]]:
    output.parent.mkdir(parents=True, exist_ok=True)
    output.unlink(missing_ok=True)
    socket.parent.mkdir(parents=True, exist_ok=True)
    socket.unlink(missing_ok=True)
    run_environment = environment.copy()
    run_environment.update(
        {
            "MINERAL_INSTANCE_MODE": "server",
            "MINERAL_INSTANCE_SOCKET": str(socket),
            "MINERAL_STARTUP_OUTPUT": str(output),
            "MINERAL_STARTUP_LABEL": "warmup-not-counted",
            "MINERAL_STARTUP_CACHE_STATE": "resident render-process and cache population",
        }
    )
    process = subprocess.Popen(
        [str(APP), str(fixture)],
        cwd=ROOT,
        env=run_environment,
        text=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
    )
    deadline = time.monotonic() + 60.0
    while time.monotonic() < deadline:
        if process.poll() is not None:
            stderr = process.stderr.read()[-2000:] if process.stderr else ""
            raise RuntimeError(
                f"resident startup server exited early ({process.returncode}): {stderr}"
            )
        if socket.exists() and output.exists():
            # The report is written in the paint callback just before the warmup
            # window closes. Let that callback unwind before opening sample one.
            time.sleep(0.02)
            return process, json.loads(output.read_text())
        time.sleep(0.005)
    process.terminate()
    process.wait(timeout=10)
    stderr = process.stderr.read()[-2000:] if process.stderr else ""
    raise RuntimeError(f"resident startup server did not become ready: {stderr}")


def stop_resident_server(
    process: subprocess.Popen[str], socket: Path, environment: dict[str, str]
) -> None:
    if process.poll() is not None:
        return
    shutdown_environment = environment.copy()
    shutdown_environment.update(
        {
            "MINERAL_INSTANCE_MODE": "client",
            "MINERAL_INSTANCE_SOCKET": str(socket),
            "MINERAL_INSTANCE_SHUTDOWN": "1",
        }
    )
    result = subprocess.run(
        [str(APP)],
        cwd=ROOT,
        env=shutdown_environment,
        text=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        timeout=10,
    )
    if result.returncode != 0:
        process.terminate()
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        process.terminate()
        process.wait(timeout=10)


def run_startup(
    fixture: Path, run_count: int, result_dir: Path, temporary: Path
) -> dict[str, object]:
    shader_cache = temporary / "shader-warm"
    warm_environment = app_environment(
        temporary / "state-warm", temporary / "cache-warm", shader_cache
    )
    first_environment = app_environment(
        temporary / "state-first-gpu",
        temporary / "cache-first-gpu",
        temporary / "shader-first-gpu",
    )
    first_gpu = startup_sample(
        fixture,
        result_dir / "startup-first-gpu.json",
        "first-gpu",
        "cold application, cold document, empty Mesa shader cache",
        first_environment,
    )

    # Populate application/Mesa caches and retain the initialized GPUI render process.
    instance_socket = temporary / "instance" / "mineral.sock"
    resident, warmup = start_resident_server(
        fixture,
        result_dir / "startup-warmup.json",
        instance_socket,
        warm_environment,
    )
    fixture.read_bytes()

    warm: list[dict[str, object]] = []
    cold: list[dict[str, object]] = []
    warm_client_environment = warm_environment.copy()
    warm_client_environment.update(
        {
            "MINERAL_INSTANCE_MODE": "client",
            "MINERAL_INSTANCE_SOCKET": str(instance_socket),
        }
    )
    try:
        for run in range(1, run_count + 1):
            warm.append(
                startup_sample(
                    fixture,
                    result_dir / f"startup-warm-{run:02}.json",
                    f"warm-{run:02}",
                    "warm filesystem/application caches and resident render process",
                    warm_client_environment,
                )
            )
            print(f"Startup warm {run}/{run_count}", flush=True)
    finally:
        stop_resident_server(resident, instance_socket, warm_environment)

    for run in range(1, run_count + 1):
        evict_file(fixture)
        cold_environment = app_environment(
            temporary / f"state-cold-{run:02}",
            temporary / f"cache-cold-{run:02}",
            shader_cache,
        )
        cold.append(
            startup_sample(
                fixture,
                result_dir / f"startup-cold-{run:02}.json",
                f"cold-{run:02}",
                "POSIX_FADV_DONTNEED document and fresh application caches; warm Mesa shader cache",
                cold_environment,
            )
        )
        print(f"Startup cold {run}/{run_count}", flush=True)

    return {
        "first_gpu": first_gpu,
        "warmup_not_counted": warmup,
        "warm": sample_distribution(warm, "elapsed_ms", 100.0),
        "cold": sample_distribution(cold, "elapsed_ms", 250.0),
        "warm_samples": warm,
        "cold_samples": cold,
    }


def start_input_stream() -> subprocess.Popen[str]:
    return subprocess.Popen(
        [str(UINPUT_CLIENT)],
        cwd=ROOT,
        text=True,
        stdin=subprocess.PIPE,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def inject(stream: subprocess.Popen[str], operation: str, first: int, second: int) -> None:
    if stream.stdin is None or stream.poll() is not None:
        raise RuntimeError("Wayland input stream closed early")
    stream.stdin.write(f"{operation} {first} {second}\n")


def drive_interaction(stream: subprocess.Popen[str], process: subprocess.Popen[str], deadline: float) -> None:
    tick = 0
    inject(stream, "move", 5000, 4000)
    inject(stream, "button", 272, 1)
    inject(stream, "button", 272, 0)
    stream.stdin.flush()
    while process.poll() is None and time.monotonic() < deadline:
        direction = 1000 if (tick // 36) % 2 == 0 else -1000
        inject(stream, "scroll", 0, direction)
        if tick % 18 == 0:
            inject(stream, "scroll", direction, 0)
        if tick % 30 == 5:
            # A real pointer selection crosses multiple shaped runs.
            inject(stream, "move", 4100, 3500)
            inject(stream, "button", 272, 1)
            for x, y in ((4500, 3700), (5000, 4000), (5600, 4400)):
                inject(stream, "move", x, y)
            inject(stream, "button", 272, 0)
        if tick % 38 == 12:
            # Focus, edit, then leave >750 ms before the next edit so autosave overlaps scrolling.
            inject(stream, "move", 5000, 4000)
            inject(stream, "button", 272, 1)
            inject(stream, "button", 272, 0)
            inject(stream, "key", 45, 1)  # KEY_X
            inject(stream, "key", 45, 0)
        stream.stdin.flush()
        tick += 1
        time.sleep(0.075)
    if process.poll() is None:
        raise RuntimeError("interaction process did not finish before its deadline")


def interaction_sample(
    input_stream: subprocess.Popen[str],
    fixture: Path,
    output: Path,
    label: str,
    seconds: float,
    warmup_ms: int,
    environment: dict[str, str],
    exercise_resize: bool,
) -> dict[str, object]:
    output.unlink(missing_ok=True)
    run_environment = environment.copy()
    run_environment.update(
        {
            "MINERAL_PERF_OUTPUT": str(output),
            "MINERAL_PERF_SECONDS": str(seconds),
            "MINERAL_PERF_WARMUP_MS": str(warmup_ms),
            "MINERAL_PERF_REFRESH_HZ": "120",
            "MINERAL_PERF_LABEL": label,
            "MINERAL_PERF_SCENARIO": "bidirectional wheel scrolling, horizontal scrolling, selection drag, editing, 750ms autosave"
            + (", and two window resizes" if exercise_resize else ""),
            "MINERAL_PERF_INPUT_SOURCE": "temporary /dev/uinput device on active Mutter session",
            "MINERAL_PERF_RESIZE": "true" if exercise_resize else "false",
        }
    )
    process = subprocess.Popen(
        [str(APP), str(fixture)],
        cwd=ROOT,
        env=run_environment,
        text=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
    )
    try:
        time.sleep(warmup_ms / 1000.0 + 0.2)
        drive_interaction(
            input_stream,
            process,
            time.monotonic() + seconds + 8.0,
        )
        stderr = process.communicate(timeout=5)[1]
    except BaseException:
        process.terminate()
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
        raise
    if process.returncode != 0 or not output.exists():
        raise RuntimeError(
            f"interaction sample {label} failed ({process.returncode}): {stderr[-2000:]}"
        )
    report = json.loads(output.read_text())
    if report["input"]["latency"]["samples"] == 0:
        raise RuntimeError(f"interaction sample {label} captured no input events")
    return report


def run_interactions(
    fixtures: dict[str, Path],
    run_count: int,
    seconds: float,
    warmup_ms: int,
    result_dir: Path,
    temporary: Path,
) -> dict[str, object]:
    input_stream = start_input_stream()
    environment = app_environment(
        temporary / "interaction-state",
        temporary / "interaction-cache",
        temporary / "interaction-shader-cache",
    )
    groups: dict[str, object] = {}
    try:
        warmup_working = fixtures["100k"].with_name("qualification-interaction-warmup.md")
        shutil.copyfile(fixtures["100k"], warmup_working)
        try:
            interaction_sample(
                input_stream,
                warmup_working,
                result_dir / "interaction-warmup.json",
                "interaction-warmup-not-counted",
                3.0,
                500,
                environment,
                True,
            )
        finally:
            warmup_working.unlink(missing_ok=True)
        for size_label, _ in SIZES:
            reports: list[dict[str, object]] = []
            for run in range(1, run_count + 1):
                working = fixtures[size_label].with_name(
                    f"qualification-{size_label}-run-{run:02}.md"
                )
                shutil.copyfile(fixtures[size_label], working)
                try:
                    report = interaction_sample(
                        input_stream,
                        working,
                        result_dir / f"interaction-{size_label}-{run:02}.json",
                        f"{size_label}-run-{run:02}",
                        seconds,
                        warmup_ms,
                        environment,
                        run == 1,
                    )
                finally:
                    working.unlink(missing_ok=True)
                reports.append(report)
                print(
                    f"Interaction {size_label} {run}/{run_count}: "
                    f"draw p99={report['draw']['p99_ms']:.3f} ms, "
                    f"input p95={report['input']['latency']['p95_ms']:.3f} ms",
                    flush=True,
                )
            groups[size_label] = interaction_summary(reports)
    finally:
        if input_stream.stdin is not None:
            input_stream.stdin.close()
        try:
            input_stream.wait(timeout=3)
        except subprocess.TimeoutExpired:
            input_stream.terminate()
            input_stream.wait(timeout=3)
    return groups


def percentile(values: list[float], quantile: float) -> float:
    ordered = sorted(values)
    return ordered[math.ceil((len(ordered) - 1) * quantile)]


def sample_distribution(
    reports: list[dict[str, object]], field: str, threshold: float
) -> dict[str, object]:
    values = [float(report[field]) for report in reports]
    p95 = percentile(values, 0.95)
    return {
        "samples": len(values),
        "min_ms": min(values),
        "median_ms": percentile(values, 0.50),
        "p95_ms": p95,
        "p99_ms": percentile(values, 0.99),
        "max_ms": max(values),
        "target_p95_ms": threshold,
        "passes": p95 <= threshold,
    }


def interaction_summary(reports: list[dict[str, object]]) -> dict[str, object]:
    worst_draw = max(float(report["draw"]["p99_ms"]) for report in reports)
    worst_input = max(
        float(report["input"]["latency"]["p95_ms"]) for report in reports
    )
    missed = sum(int(report["presentation"]["missed_deadlines"]) for report in reports)
    opportunities = sum(
        int(report["presentation"]["deadline_opportunities"]) for report in reports
    )
    application_stalls = sum(
        int(report["application_stalls_at_least_25_ms"]) for report in reports
    )
    presentation_intervals_at_least_25_ms = sum(
        int(report["presentation"]["intervals_at_least_25_ms"]) for report in reports
    )
    missed_percent = 100.0 * missed / opportunities if opportunities else 0.0
    return {
        "runs": len(reports),
        "worst_run_draw_p99_ms": worst_draw,
        "target_draw_p99_ms": 6.0,
        "worst_run_input_p95_ms": worst_input,
        "target_input_p95_ms": 16.7,
        "missed_deadlines": missed,
        "deadline_opportunities": opportunities,
        "missed_deadline_percent": missed_percent,
        "target_missed_deadline_percent_exclusive": 0.1,
        "application_stalls_at_least_25_ms": application_stalls,
        "presentation_intervals_at_least_25_ms": presentation_intervals_at_least_25_ms,
        "passes": worst_draw <= 6.0
        and worst_input <= 16.7
        and missed_percent < 0.1
        and application_stalls == 0,
        "reports": reports,
    }


def command_text(command: list[str]) -> str | None:
    try:
        return subprocess.run(
            command,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=10,
            check=False,
        ).stdout.strip()
    except (OSError, subprocess.TimeoutExpired):
        return None


def environment_report() -> dict[str, object]:
    display_state = command_text(
        [
            "gdbus",
            "call",
            "--session",
            "--dest",
            "org.gnome.Mutter.DisplayConfig",
            "--object-path",
            "/org/gnome/Mutter/DisplayConfig",
            "--method",
            "org.gnome.Mutter.DisplayConfig.GetCurrentState",
        ]
    )
    return {
        "captured_at": dt.datetime.now(dt.timezone.utc).isoformat(),
        "uname": command_text(["uname", "-a"]),
        "rustc": command_text(["rustc", "-Vv"]),
        "weston": command_text(["weston", "--version"]),
        "wayland_display": os.environ.get("WAYLAND_DISPLAY"),
        "desktop": os.environ.get("XDG_CURRENT_DESKTOP"),
        "session_type": os.environ.get("XDG_SESSION_TYPE"),
        "power_profile": Path("/sys/firmware/acpi/platform_profile").read_text().strip()
        if Path("/sys/firmware/acpi/platform_profile").exists()
        else None,
        "mutter_display_state": display_state,
    }


def fixture_report(fixtures: dict[str, Path]) -> dict[str, object]:
    return {
        label: {
            "path": str(path.relative_to(ROOT)),
            "bytes": path.stat().st_size,
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        }
        for label, path in fixtures.items()
    }


def main() -> None:
    args = arguments()
    if not args.skip_build:
        build()
    fixtures = generate_fixtures()
    result_dir = PERFORMANCE / "results"
    result_dir.mkdir(parents=True, exist_ok=True)
    report: dict[str, object] = {
        "schema_version": 1,
        "method": (
            "release build; main-entry-to-editable-frame startup; direct active Mutter "
            "Wayland display; temporary kernel uinput device supplying real libinput events"
        ),
        "environment": environment_report(),
        "fixtures": fixture_report(fixtures),
        "requested": {
            "startup_runs_per_scenario": args.startup_runs,
            "interaction_runs_per_size": args.interaction_runs,
            "interaction_seconds": args.seconds,
            "warmup_ms": args.warmup_ms,
        },
    }
    with tempfile.TemporaryDirectory(prefix="mineral-qualification-") as temporary_name:
        temporary = Path(temporary_name)
        if not args.skip_startup:
            report["startup_100k"] = run_startup(
                fixtures["100k"], args.startup_runs, result_dir, temporary
            )
        if not args.skip_interaction:
            report["interaction"] = run_interactions(
                fixtures,
                args.interaction_runs,
                args.seconds,
                args.warmup_ms,
                result_dir,
                temporary,
            )

    core_report = json.loads((PERFORMANCE / "core-2026-09-06-10m.json").read_text())
    core_p95 = float(core_report["open_prepare_ms"]["p95"])
    report["core_open_prepare_10m"] = {
        "p95_ms": core_p95,
        "target_p95_ms": 1000.0,
        "passes": core_p95 <= 1000.0,
        "source": "performance/core-2026-09-06-10m.json",
    }
    gates = [report["core_open_prepare_10m"]["passes"]]
    if "startup_100k" in report:
        gates.extend(
            [
                report["startup_100k"]["warm"]["passes"],
                report["startup_100k"]["cold"]["passes"],
            ]
        )
    if "interaction" in report:
        gates.extend(group["passes"] for group in report["interaction"].values())
    report["passes_all_executed_gates"] = all(gates)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(f"Qualification report written to {args.output}", flush=True)
    if not report["passes_all_executed_gates"]:
        raise SystemExit("one or more release gates failed")


if __name__ == "__main__":
    main()
