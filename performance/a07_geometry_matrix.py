#!/usr/bin/env python3
"""Run the complete native A07 width and display-scale matrix."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
WIDTHS = (480, 640, 799, 800, 999, 1000, 1440)
SCALES = (120, 150, 180, 240)
REPRESENTATIVES = {(480, 150), (799, 180), (800, 150), (1440, 240)}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path,
                        default=ROOT / "target/release/tachyon")
    parser.add_argument("--output", type=Path,
                        default=ROOT / "performance/layout-previews/a07-native-geometry-matrix.json")
    parser.add_argument("--height", type=int, default=800)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    binary_sha256 = hashlib.sha256(binary.read_bytes()).hexdigest()
    cases = []

    with tempfile.TemporaryDirectory(prefix="tachyon-a07-matrix-") as temporary:
        temporary = Path(temporary)
        for width in WIDTHS:
            for scale in SCALES:
                stem = f"a07-{width}-{scale}"
                image = temporary / f"{stem}.png"
                log = temporary / f"{stem}.log"
                command = [
                    "python3", str(ROOT / "performance/capture-layout.py"),
                    "--fixture", "41-find-document.md",
                    "--binary", str(binary),
                    "--width", str(width),
                    "--height", str(args.height),
                    "--scale", str(scale),
                    "--source-unchanged-check",
                    "--atspi-active",
                    "--layout-trace", "details",
                    "--a07-geometry-check",
                    "--output", str(image),
                    "--log-output", str(log),
                ]
                completed = subprocess.run(
                    command, cwd=ROOT, capture_output=True, text=True, timeout=120
                )
                if completed.returncode:
                    failure_log = output.with_name(f"{output.stem}-failure-{width}-{scale}.log")
                    failure_log.write_text(
                        completed.stdout + "\n--- stderr ---\n" + completed.stderr
                        + "\n--- weston ---\n" + (log.read_text() if log.exists() else "")
                    )
                    raise RuntimeError(
                        f"A07 matrix case {width}px/{scale / 1.2:.0f}% failed; see {failure_log}"
                    )
                report = json.loads(image.with_suffix(".a07.json").read_text())
                if report.get("binary_sha256") != binary_sha256 or not report.get("passes"):
                    raise RuntimeError(f"A07 matrix case {width}/{scale} produced invalid evidence")
                edit = report.get("native_edit", {})
                if not (
                    edit.get("single_x_insertion_within_target")
                    and edit.get("only_lossless_markdown_escaping_besides_insertion")
                    and edit.get("undo_restores_exact_bytes")
                    and report.get("geometry_after_undo", {}).get("passes")
                    and report.get("source_unchanged")
                ):
                    raise RuntimeError(
                        f"A07 matrix case {width}/{scale} lacks exact native edit evidence"
                    )
                cases.append(report)
                if (width, scale) in REPRESENTATIVES:
                    destination = output.with_name(f"{output.stem}-{width}-{scale}.png")
                    shutil.copyfile(image, destination)
                print(f"A07 {width}px @ {scale / 1.2:.0f}%: pass", flush=True)

    expected = {(width, scale) for width in WIDTHS for scale in SCALES}
    observed = {
        (case["baseline"]["width"], case["baseline"]["display_scale_120"])
        for case in cases
    }
    report = {
        "passes": len(cases) == len(expected) and observed == expected
        and all(case["passes"] for case in cases),
        "binary_sha256": binary_sha256,
        "fixture": "41-find-document.md",
        "height": args.height,
        "widths": list(WIDTHS),
        "display_scales_120": list(SCALES),
        "display_scale_percentages": [scale / 1.2 for scale in SCALES],
        "case_count": len(cases),
        "native_edit_cases": sum(
            case.get("native_edit", {}).get("single_x_insertion_within_target")
            and case.get("native_edit", {}).get("undo_restores_exact_bytes")
            for case in cases
        ),
        "representative_cases": [list(case) for case in sorted(REPRESENTATIVES)],
        "cases": cases,
    }
    output.write_text(json.dumps(report, indent=2) + "\n")
    if not report["passes"]:
        raise RuntimeError("Complete A07 matrix coverage was not produced")
    print(output)


if __name__ == "__main__":
    main()
