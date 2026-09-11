#!/usr/bin/env python3
"""Capture the native app on an isolated Weston output, with real Wayland input.

Example: python3 performance/capture-layout.py --fixture 02-list-arrangements.md
"""

import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time


ROOT = Path(__file__).resolve().parents[1]


def generated_accessible_root_count(source):
    sections = sum(
        line.startswith(b"## Field section ") and line[17:].isdigit()
        for line in source.splitlines()
    )
    return 4 + sections * 33


def stage_source_document(source, fixture_directory):
    """Stage exact holdout bytes; never give the editable app the original path.

    Only the selected Markdown is copied. Relative images/resources are not
    imported from the user's workspace and must be qualified separately.
    """
    source = Path(source).resolve(strict=True)
    contents = source.read_bytes()
    directory = fixture_directory / "holdout"
    directory.mkdir()
    destination = directory / source.name
    destination.write_bytes(contents)
    return str(destination.relative_to(fixture_directory))


def source_origin_evidence(source, staged_bytes):
    """Keep origin drift separate from the editable private copy's fidelity.

    `source` is resolved before staging. An external edit/deletion is evidence
    about comparability, not proof that the isolated app changed the original.
    """
    staged_hash = hashlib.sha256(staged_bytes).hexdigest()
    result = dict(path=str(source), staged_sha256=staged_hash,
                  resource_scope="selected_markdown_only")
    try:
        current_hash = hashlib.sha256(source.read_bytes()).hexdigest()
    except OSError as error:
        result.update(state_at_check="unavailable", current_sha256=None,
                      error_type=type(error).__name__)
    else:
        result.update(state_at_check="unchanged" if current_hash == staged_hash else "changed",
                      current_sha256=current_hash)
    return result


def stage_source_resources(source, staged, private_root, directories):
    """Copy explicitly selected resource directories at their authored paths."""
    evidence = []
    for directory in directories:
        directory = directory.resolve(strict=True)
        relative = Path(os.path.relpath(directory, source.parent))
        destination = (staged.parent / relative).resolve()
        if not destination.is_relative_to(private_root.resolve()) or destination == private_root.resolve():
            raise ValueError("Resource directory escapes the private staging root")
        if not directory.is_dir():
            raise ValueError("Source resource must be a directory")
        shutil.copytree(directory, destination)
        hashes = {str(path.relative_to(destination)): hashlib.sha256(path.read_bytes()).hexdigest()
                  for path in sorted(destination.rglob("*")) if path.is_file()}
        evidence.append(dict(source=str(directory), relative_path=str(relative), sha256=hashes))
    return evidence


def measured_startup_ready(reports, canvas_width):
    """Inspect the latest publication, not an obsolete correct-sized result."""
    latest = next((report for report in reversed(reports) if report.get("committed")), None)
    return bool(latest
                and latest.get("scope") == "whole_document"
                and not latest.get("planner_failed", True)
                and latest.get("unresolved_image_dimensions") == 0
                and abs(latest.get("canvas_width", 0) - canvas_width) < 1)


def check_cached_geometry(reports, expected_samples):
    """Gate same-environment warm replans of a cache-resident document.

    This is deliberately opt-in: oversized segments and cache eviction have
    valid uncached fallbacks. Counts prove reuse, not bounded whole-worker work.
    """
    warm = [report for report in reports if report["explicitly_requested"] and report["committed"]]
    if len(warm) != expected_samples:
        raise RuntimeError(f"Expected {expected_samples} committed warm replans, got {len(warm)}")
    for report in warm:
        geometry = next(stage["measurements"] for stage in report["stages"]
                        if stage["name"] == "geometry_and_rendered_extensions")
        published = geometry.get("published_geometry_reuses", 0)
        reused = ((published == 1 and geometry["geometry_requests"] == 0
                   and geometry["geometry_cache_hits"] == 0)
                  or (published == 0 and geometry["geometry_requests"] > 0
                      and geometry["geometry_cache_hits"] == geometry["geometry_requests"]))
        if (not reused
                or any(geometry[name] != 0 for name in (
                    "segments_laid_out", "geometry_cache_evictions", "wrap_requests", "shaping_calls"))
                or report["anchor_displacement_at_commit_px"] != 0):
            raise RuntimeError(f"Warm geometry reuse/anchor regression in sample {report['sequence']}: "
                               f"{geometry}; anchor={report['anchor_displacement_at_commit_px']}")


SELECTION_KEY_CODES = {"up": 103, "down": 108, "home": 102, "end": 107, "left": 105, "right": 106}
SELECTION_KEYS = [*SELECTION_KEY_CODES, *("shift-" + key for key in SELECTION_KEY_CODES),
                  "ctrl-left", "ctrl-right", "ctrl-shift-left", "ctrl-shift-right", "alt-enter"]


def selection_key_events(key):
    if key == "alt-enter":
        return [(56, 1), (28, 1), (28, 0), (56, 0)]
    parts = key.split("-")
    modifiers = ([29] if "ctrl" in parts else []) + ([42] if "shift" in parts else [])
    code = SELECTION_KEY_CODES[parts[-1]]
    return [(modifier, 1) for modifier in modifiers] + [(code, 1), (code, 0)] + [
        (modifier, 0) for modifier in reversed(modifiers)]


def single_inserted_codepoint(original, candidate):
    """Return the one provisional codepoint inserted into an existing label.

    The private US-International keymap exposes its dead acute as one
    provisional codepoint inside the surrounding accessible text. This oracle
    deliberately rejects replacements, deletions, and multi-character changes.
    """
    if len(candidate) != len(original) + 1:
        return None
    for offset in range(len(candidate)):
        if candidate[:offset] == original[:offset] and candidate[offset + 1:] == original[offset:]:
            return offset, candidate[offset]
    return None


MARKDOWN_ESCAPABLE_BYTES = frozenset(b'!"#$%&\'()*+,-./:;<=>?@[\\]^_`{|}~')


def unescape_markdown_punctuation(source):
    """Project generated escapes in a plain Markdown text node back to text.

    Editing a rich paragraph may conservatively escape ASCII punctuation when
    serializing that node. The native edit oracle compares the targeted text
    after this lossless projection, while exact undo still compares all source
    bytes and catches unrelated mutations.
    """
    projected = bytearray()
    offset = 0
    while offset < len(source):
        if (source[offset] == ord("\\") and offset + 1 < len(source)
                and source[offset + 1] in MARKDOWN_ESCAPABLE_BYTES):
            projected.append(source[offset + 1])
            offset += 2
        else:
            projected.append(source[offset])
            offset += 1
    return bytes(projected)


def check_copied_text(copied, markers, expected=None):
    positions = [copied.find(marker) for marker in markers]
    marker_order = all(copied.count(marker) == 1 for marker in markers) and positions == sorted(positions)
    exact = expected is None or copied == expected
    return {
        "markers": markers,
        "positions": positions,
        "source_order": marker_order,
        "exact_clipboard": exact if expected is not None else None,
        "passes": marker_order and exact,
    }


def inserted_marker_matches(original, edited, marker, insertion):
    """Locate an insertion in one unique source marker, including its endpoints.

    This is a target-location oracle, not whole-file fidelity proof. Callers
    retain their independent exact edited-source and exact undo assertions.
    """
    semantic_edited = unescape_markdown_punctuation(edited)
    return bool(marker and insertion and original.count(marker) == 1 and any(
                    semantic_edited.count(marker[:offset] + insertion + marker[offset:]) == 1
                    for offset in range(len(marker) + 1)))


def check_idle_document_frames(frames):
    """Fixture 57: positive timeline evidence, not merely two equal blank frames."""
    from PIL import ImageChops
    if len(frames) != 2 or any(frame.size != (1150, 880) for frame in frames):
        raise ValueError("Expected two document crops from a 1600x1200 frame")
    changed = sum(pixel != (0, 0, 0) for i, pixel in enumerate(ImageChops.difference(*frames).getdata())
                  if not (375 <= i % 1150 < 440 and 65 <= i // 1150 < 135))
    # Second authored date's 8 px dot, measured from the board specimen's
    # published slot (606,334); it is outside the ignored pointer rectangle.
    marker = frames[0].getpixel((206, 74)) == (63, 98, 71)
    return dict(passes=changed == 0 and marker,
                changed_document_pixels=changed, idle_timeline_marker=marker)


def check_idle_definition_frames(frames):
    """Fixture 63: require aligned glossary text AND a measured numbered row.

    Compare the actual document after startup with the same document after
    pointer movement. Equal blank or provisional stacked frames cannot pass.
    Bounds are the reviewed 1600x1200, 100% specimen, not a generated golden.
    """
    from PIL import ImageChops
    if len(frames) != 2 or any(frame.size != (1600, 1200) for frame in frames):
        raise ValueError("Expected two 1600x1200 definition frames")
    markers = {
        "first_term": (276, 332, 325, 352),
        "first_description": (388, 332, 540, 352),
        "second_term": (276, 420, 345, 440),
        "second_description": (388, 420, 510, 440),
    }
    ink = {name: sum(max(pixel) < 180 for pixel in frames[0].crop(bounds).getdata())
           for name, bounds in markers.items()}
    # The middle authored number cannot occur here in the provisional stack.
    number_ink = sum(pixel == (63, 98, 71)
                     for pixel in frames[0].crop((730, 590, 765, 625)).getdata())
    composition = all(count >= 80 for count in ink.values()) and number_ink >= 10
    crop = (256, 260, 1550, 1140)
    difference = ImageChops.difference(frames[0].crop(crop), frames[1].crop(crop))
    changed = sum(pixel != (0, 0, 0) for index, pixel in enumerate(difference.getdata())
                  if not (519 <= index % 1294 < 584 and 65 <= index // 1294 < 135))
    return dict(passes=composition and changed == 0, changed_document_pixels=changed,
                aligned_definition_ink=ink, numbered_row_ink=number_ink,
                idle_definition_composition=composition)


def main():
    parser = argparse.ArgumentParser()
    source_group = parser.add_mutually_exclusive_group()
    source_group.add_argument("--fixture", default="02-list-arrangements.md")
    source_group.add_argument("--source-document", type=Path,
                              help="copy exact Markdown into the private session; relative resources are not copied")
    parser.add_argument("--source-resource-dir", type=Path, action="append", default=[],
                        help="with --source-document, copy this resource directory preserving its relative path")
    parser.add_argument("--recorded-bugs-check", choices=["cards", "table-flow", "insertion", "knobs", "table-history"],
                        help="verify one bounded Crusty regression in the private native session")
    parser.add_argument("--atspi-check", action="store_true",
                        help="fixture 27: inspect native document semantics on a private accessibility bus")
    parser.add_argument("--refresh-rate", type=int, default=120000,
                        help="isolated compositor refresh in mHz; Weston 14 uses 0 for capture-only repaint, not idle rendering")
    parser.add_argument("--startup-capture-frames", type=int, default=0,
                        help="explicitly drive 1–12 startup frames in capture-only mode; never evidence of idle rendering or fps")
    parser.add_argument("--atspi-active", action="store_true",
                        help="activate and verify a private AT-SPI tree during ordinary layout/performance checks")
    parser.add_argument("--a07-geometry-check", action="store_true",
                        help="fixture 41: verify native scale, geometry, anchors and responsive Files/Outline")
    parser.add_argument("--atspi-activation-sample", action="store_true",
                        help="sample generated-document roots instead of exhaustively traversing a large active tree")
    parser.add_argument("--atspi-math-check", action="store_true",
                        help="fixture 40: verify native MathML structure, token order and exact source")
    parser.add_argument("--atspi-html-check", action="store_true",
                        help="fixture 28: verify HTML text, native disclosures and source preservation")
    parser.add_argument("--atspi-gallery-check", action="store_true",
                        help="fixture 32: verify image roles/order and native heading-link activation")
    parser.add_argument("--atspi-editorial-check", action="store_true",
                        help="fixture 47: verify automatic composition traversal, focus and geometry")
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/mineral-markdown",
                        help="app binary to verify (release by default)")
    parser.add_argument("--width", type=int, default=1440)
    parser.add_argument("--height", type=int, default=1000)
    parser.add_argument("--scale", type=int, default=120)
    parser.add_argument("--zoom-steps", type=int, default=0,
                        help="native Ctrl+=/- steps; 10 verifies 200%% document text")
    parser.add_argument("--select-before-zoom", action="store_true",
                        help="select all before zoom to exercise selected layout publication")
    parser.add_argument("--startup-wait", type=float, default=3,
                        help="allow cold native font/HTML initialization before sending input (seconds)")
    parser.add_argument("--reflow-wait", type=float, default=1,
                        help="allow asynchronous cold reflow after zoom before capture (seconds; not a latency measurement)")
    parser.add_argument("--edit-check", action="store_true",
                        help="type x at the caret, verify autosave, undo and verify exact source restoration")
    parser.add_argument("--table-resize-check", action="store_true",
                        help="fixture 82: use --select coordinates to verify provisional resize, Escape, saved width and undo")
    parser.add_argument("--task-toggle-check", metavar="LABEL",
                        help="click --select coordinates; verify only this complete task line's marker changes, then undo")
    parser.add_argument("--edit-compose-check", metavar="PREEDIT_TEXT",
                        help="compose é instead of x; require this exact provisional native text and no preedit autosave")
    parser.add_argument("--edit-compose-cancel-check", action="store_true",
                        help="cancel preedit with Escape, verify restored selection, then restart and commit")
    parser.add_argument("--edit-within", help="require native typing to insert x inside this unique source substring")
    parser.add_argument("--edit-expect", help="require the complete autosaved Markdown to equal this text after editing")
    parser.add_argument("--edit-preserve", nargs="+", metavar="TEXT",
                        help="require these unique source fragments to remain byte-exact during native editing")
    parser.add_argument("--edit-paste", help="paste this bounded synthetic text instead of typing x (private Wayland seat)")
    parser.add_argument("--edit-markdown", action="store_true",
                        help="use native Paste as Markdown for --edit-paste (structural edit path)")
    parser.add_argument("--edit-idle-seconds", type=float, default=0,
                        help="wait after typing and capture the still-focused document before undo")
    parser.add_argument("--edit-zoom-steps", type=int, default=0,
                        help="change text zoom after editing, wait for reflow and capture before undo")
    parser.add_argument("--edit-blur", nargs=2, type=int, metavar=("X", "Y"),
                        help="move the caret out of the edited row and capture its automatic reconsideration")
    parser.add_argument("--scroll", type=int, default=0)
    parser.add_argument("--horizontal-scroll", nargs=3, type=int, metavar=("X", "Y", "DELTA"),
                        help="send native horizontal wheel input at a document point")
    parser.add_argument("--copy-order-check", nargs="+", metavar="TEXT",
                        help="select all and copy natively; require each unique marker once in the supplied order")
    parser.add_argument("--idle-layout-check", action="store_true",
                        help="fixture 57/63: require measured startup composition before any pointer input")
    parser.add_argument("--copy-expect-file", type=Path,
                        help="also require the native plain-text clipboard to equal this UTF-8 golden file")
    parser.add_argument("--convert-html-check", nargs=2, type=int, metavar=("X", "Y"),
                        help="fixtures 10/23/29: convert/edit HTML; verify formatting, typing and exact undo")
    parser.add_argument("--html-edit-here", action="store_true",
                        help="right-click the conversion coordinates and invoke Edit text here")
    parser.add_argument("--html-direct-edit", action="store_true",
                        help="click rendered text without conversion, then type; one undo must restore original HTML")
    parser.add_argument("--html-compose-check", action="store_true",
                        help="use an isolated US International dead-key preedit and commit é instead of typing x")
    parser.add_argument("--html-link-check", nargs=2, type=int, metavar=("X", "Y"),
                        help="fixture 10: capture HTML link hover/menu, copy its address and verify unchanged source")
    parser.add_argument("--html-clipboard-check", action="store_true",
                        help="fixtures 44/128: native selected Markdown/full-root HTML MIME and first-edit undo")
    parser.add_argument("--follow-link", nargs=2, type=int, metavar=("X", "Y"),
                        help="Ctrl-click a synthetic heading/local link; verify destination typing and exact undo")
    parser.add_argument("--html-anchor-check", nargs=2, type=int, metavar=("X", "Y"),
                        help="fixture 26: follow an HTML ID and verify its closed body is revealed without source changes")
    parser.add_argument("--html-anchor-keyboard", action="store_true",
                        help="activate fixture 26's link using Alt+Enter instead of Ctrl-click")
    parser.add_argument("--follow-fixture", help="destination fixture for a local link (default: current fixture)")
    parser.add_argument("--follow-marker", help="unique destination substring that must receive x at its start")
    parser.add_argument("--disclosure-check", nargs=2, type=int, metavar=("X", "Y"),
                        help="fixtures 20/22: open by mouse, close/open by Enter/Space; verify body geometry and unchanged source")
    parser.add_argument("--html-edit-within",
                        help="require the converted insertion to lie inside this unique text")
    parser.add_argument("--scroll-steps", type=int, default=1)
    parser.add_argument("--scroll-settle-seconds", type=float, default=0.6,
                        help="wait before collecting static AT-SPI/screenshot geometry after scroll; not a latency measurement")
    parser.add_argument("--select", nargs=4, type=int, metavar=("X1", "Y1", "X2", "Y2"))
    parser.add_argument("--select-hold-seconds", type=float, default=0,
                        help="hold the selection pointer stationary before release to exercise edge autoscroll")
    parser.add_argument("--selection-keys", nargs="+",
                        choices=SELECTION_KEYS,
                        help="native navigation keys after selecting/clicking and before copy or editing")
    parser.add_argument("--copy-selected", help="require native drag selection/copy to match this text without changing source")
    parser.add_argument("--hover", nargs=2, type=int, metavar=("X", "Y"))
    parser.add_argument("--coast-check", choices=["wheel", "continuous"])
    parser.add_argument("--source-unchanged-check", action="store_true",
                        help="verify the copied fixture remains byte-identical after view interactions and autosave delay")
    parser.add_argument("--startup-ready-width", type=float,
                        help="before native input, require a committed measured canvas of this width (layout trace required)")
    parser.add_argument("--find-check", action="store_true", help="fixture 41: native find, copy, disclosure reveal, edit and undo")
    parser.add_argument("--layout-recovery-check", choices=["panic", "timeout"],
                        help="fixture 43: native faults; requires an explicit layout-validation binary")
    parser.add_argument("--resize-check", choices=["reading", "editing"],
                        help="verify supported source-preserving compositions with a layout-validation binary")
    parser.add_argument("--resize-axis", choices=["width", "height"], default="width",
                        help="change only width or height during a native resize check")
    parser.add_argument("--resize-target", choices=["default", "specification-table", "specification-code",
                                                   "timeline-body", "timeline-code", "timeline-table", "timeline-nested",
                                                   "record-body", "record-header"], default="default",
                        help="specification pair, rich events or paired records: select an exact native edit target")
    parser.add_argument("--resize-burst", action="store_true",
                        help="send alternating resize requests without settling between them (requires trace)")
    parser.add_argument("--resize-continue-edit", action="store_true",
                        help="fixture122: continue typing and navigate cells after restoring the window; undo each edit")
    parser.add_argument("--appearance", choices=["light", "dark"],
                        help="provide OS appearance through a private settings portal")
    parser.add_argument("--appearance-cycle", action="store_true",
                        help="switch OS appearance and back; verify pixels, geometry, identity and source")
    parser.add_argument("--boundary-check", action="store_true",
                        help="fixture 47: verify automatic-only hysteresis at an adjacent-width boundary")
    parser.add_argument("--font-reflow-check", action="store_true",
                        help="fixture 48: verify measured body-font reflow with a layout-validation binary")
    parser.add_argument("--rtl-check", action="store_true",
                        help="fixture 48: verify native RTL hit testing and visual arrow movement")
    parser.add_argument("--adaptive-check", choices=["lists", "rows", "tables"],
                        help="fixtures 06/11/12: verify automatic geometry and exact semantic order")
    parser.add_argument("--adaptive-profile", choices=["wide", "medium", "narrow"],
                        help="expected automatic-layout width profile for --adaptive-check")
    parser.add_argument("--chapter-identity-check", action="store_true",
                        help="fixture 49: verify chapter barriers and duplicate identities through edit/undo")
    parser.add_argument("--html-effects-check", action="store_true",
                        help="fixture 50: prove HTML measurement/disclosures make no active requests")
    parser.add_argument("--html-images-check", action="store_true",
                        help="fixture 30: require decoded image colors and unchanged source after native loading")
    parser.add_argument("--window-controls-check", action="store_true",
                        help="Verify keyboard maximize, restore, minimize and close in a private desktop shell")
    parser.add_argument("--unsaved-close-check", choices=("save", "discard"),
                        help="Verify title-bar Close offers Cancel and safely saves/discards a private unsaved edit")
    parser.add_argument("--recovery-controls-check", choices=("reload", "overwrite"),
                        help="Restore a private recovery draft through the status bar, then reload or overwrite")
    parser.add_argument("--titlebar-check", action="store_true",
                        help="capture the titlebar and require its trailing close button to work inside the window")
    parser.add_argument("--perf-seconds", type=float, default=0)
    parser.add_argument("--perf-sweep-seconds", type=float, default=0,
                        help="sweep the full accessible document per direction (continuous input, active AT)")
    parser.add_argument("--perf-warm-sweep", action="store_true",
                        help="traverse down and back before measuring the document sweep")
    parser.add_argument("--perf-input", choices=["wheel", "continuous"], default="wheel",
                        help="native wheel impulses or raw continuous deltas for the performance run")
    parser.add_argument("--generated-bytes", type=int, default=0)
    parser.add_argument("--output", type=Path, default=ROOT / "performance/results/layout.png")
    parser.add_argument("--log-output", type=Path, help="retain native app/compositor diagnostics")
    parser.add_argument("--layout-trace", choices=["summary", "details"],
                        help="record content-free staged planning diagnostics; require a committed measured plan")
    parser.add_argument("--layout-samples", type=int, default=0,
                        help="request 0–30 same-environment replans via the developer keyboard command")
    parser.add_argument("--cached-geometry-check", action="store_true",
                        help="require all warm segment geometry to hit cache, with no shaping or anchor movement")
    parser.add_argument("--layout-probes", type=int, nargs="+", metavar="XY",
                        help="pairs of flat-surface pixel coordinates that must retain their colors through selection/edit/undo")
    parser.add_argument("--probe-color", type=int, nargs=3, metavar=("R", "G", "B"),
                        help="required surface color at layout probes (prevents an already-missing layout from passing)")
    args = parser.parse_args()
    if args.recovery_controls_check and (args.fixture != "41-find-document.md" or args.scale != 120
                                        or args.zoom_steps or args.unsaved_close_check
                                        or args.window_controls_check or args.titlebar_check
                                        or args.source_document):
        parser.error("recovery-controls-check requires private fixture41 separately at scale120 and default zoom")
    if args.unsaved_close_check and (args.window_controls_check or args.titlebar_check
                                    or args.fixture != "41-find-document.md"
                                    or args.scale != 120 or args.zoom_steps):
        parser.error("unsaved-close-check requires fixture41 separately at scale120 and default zoom")
    if args.window_controls_check and (args.titlebar_check or args.scale != 120
                                      or args.zoom_steps or args.width < 1600
                                      or args.fixture != "41-find-document.md"
                                      or args.appearance or args.appearance_cycle):
        parser.error("window-controls-check requires fixture41 separately at scale120, default appearance/zoom and width>=1600")
    if Path(args.fixture).is_absolute() or ".." in Path(args.fixture).parts:
        parser.error("--fixture must stay inside the private fixture directory; use --source-document for real files")
    if args.source_resource_dir and not args.source_document:
        parser.error("--source-resource-dir requires --source-document")
    if args.source_document:
        if args.generated_bytes or not args.source_document.is_file():
            parser.error("--source-document requires an existing file and cannot combine with --generated-bytes")
        # Ensure fixture-specific interaction oracles cannot mistake the
        # default fixture name for this arbitrary holdout.
        args.fixture = "holdout/" + args.source_document.resolve().name
    if args.recorded_bugs_check and (
            args.fixture != ("122-paired-records.md" if args.recorded_bugs_check == "table-history" else
                             f"crusty-{'insertion' if args.recorded_bugs_check == 'knobs' else args.recorded_bugs_check}.md")
            or args.perf_seconds or args.edit_check or args.select):
        parser.error("recorded bug checks require their matching crusty fixture and a separate run")
    if args.refresh_rate < 0:
        parser.error("--refresh-rate must be nonnegative")
    if not 0 <= args.startup_capture_frames <= 12 or args.startup_capture_frames and args.refresh_rate != 0:
        parser.error("--startup-capture-frames requires --refresh-rate=0 and a count from 1 to 12")
    if args.startup_capture_frames and args.idle_layout_check:
        parser.error("capture-driven startup cannot be used as idle-layout evidence")
    if args.startup_ready_width is not None and (not args.layout_trace or not 0 < args.startup_ready_width <= args.width):
        parser.error("--startup-ready-width requires layout trace and a valid canvas width")
    if args.perf_seconds and args.refresh_rate != 120000:
        parser.error("performance evidence uses the configured 120 Hz clock; use --refresh-rate=120000")
    if args.appearance and (args.resize_check or args.boundary_check or args.font_reflow_check):
        parser.error("appearance pixel checks require a maximized kiosk window; run separately from desktop-shell resize checks")
    if args.layout_recovery_check and (args.fixture != "43-layout-recovery.md" or not args.layout_trace or args.perf_seconds or args.edit_check or args.find_check):
        parser.error("layout recovery requires fixture 43 and --layout-trace separately from performance/edit/find checks")
    if args.resize_check and (
            args.fixture not in ("01-field-notes.md", "03-technical-reference.md", "47-editorial-composition.md", "79-technical-sections.md", "106-extended-table.md", "107-extended-code.md", "116-code-led-explanations.md", "117-table-led-resize.md", "118-opening-overview.md", "119-rich-timelines.md", "122-paired-records.md") or args.perf_seconds or args.edit_check
            or args.find_check or args.layout_recovery_check or args.atspi_editorial_check
            or args.boundary_check):
        parser.error("resize checks require fixture 01, 03, 47, 79, 106, 107, 116, 117, 118, 119 or 122 separately from edit/performance/accessibility checks")
    if args.boundary_check and (
            args.fixture != "47-editorial-composition.md" or args.layout_trace != "details"
            or args.perf_seconds or args.edit_check or args.find_check
            or args.layout_recovery_check or args.resize_check or args.font_reflow_check
            or args.adaptive_check or args.chapter_identity_check or args.html_effects_check
            or args.atspi_check or args.atspi_html_check or args.atspi_gallery_check
            or args.atspi_editorial_check or args.atspi_math_check or args.atspi_active):
        parser.error("boundary checks require fixture 47, detailed trace, and a separate native run")
    if args.font_reflow_check and (
            args.fixture != "48-typography-direction-overflow.md"
            or args.layout_trace != "details" or args.perf_seconds or args.edit_check
            or args.find_check or args.layout_recovery_check or args.resize_check):
        parser.error("font reflow requires fixture 48, --layout-trace details, and a separate run")
    if args.rtl_check and (
            args.fixture != "48-typography-direction-overflow.md"
            or args.perf_seconds or args.edit_check or args.find_check
            or args.layout_recovery_check or args.resize_check or args.font_reflow_check
            or args.boundary_check or args.adaptive_check or args.chapter_identity_check
            or args.html_effects_check or args.atspi_check or args.atspi_html_check
            or args.atspi_gallery_check or args.atspi_editorial_check
            or args.atspi_math_check or args.atspi_active):
        parser.error("RTL checks require fixture 48 and a separate native run")
    adaptive_fixtures = {
        "lists": "06-measured-list-fit.md",
        "rows": "11-measured-rows.md",
        "tables": "12-measured-tables.md",
    }
    if bool(args.adaptive_check) != bool(args.adaptive_profile):
        parser.error("--adaptive-check and --adaptive-profile must be supplied together")
    if args.adaptive_check and (
            args.fixture != adaptive_fixtures[args.adaptive_check]
            or args.perf_seconds or args.edit_check or args.find_check
            or args.layout_recovery_check or args.resize_check or args.font_reflow_check
            or args.atspi_check or args.atspi_html_check or args.atspi_gallery_check
            or args.atspi_editorial_check or args.atspi_math_check or args.atspi_active):
        parser.error("adaptive checks require their matching fixture and a separate native run")
    if args.chapter_identity_check and (
            args.fixture != "49-chapter-identity.md" or args.perf_seconds or args.edit_check
            or args.find_check or args.layout_recovery_check or args.resize_check
            or args.font_reflow_check or args.adaptive_check or args.atspi_check
            or args.atspi_html_check or args.atspi_gallery_check
            or args.atspi_editorial_check or args.atspi_math_check or args.atspi_active):
        parser.error("chapter identity requires fixture 49 and a separate native run")
    if args.html_effects_check and (
            args.fixture != "50-inert-html-effects.md" or args.layout_trace != "details"
            or args.layout_samples < 2 or args.perf_seconds or args.edit_check
            or args.find_check or args.layout_recovery_check or args.resize_check
            or args.font_reflow_check or args.adaptive_check or args.chapter_identity_check
            or args.boundary_check
            or args.atspi_check or args.atspi_html_check or args.atspi_gallery_check
            or args.atspi_editorial_check or args.atspi_math_check or args.atspi_active):
        parser.error("HTML effects require fixture 50, detailed trace, at least two samples, and a separate run")
    if not 0 <= args.layout_samples <= 30 or (args.layout_samples and not args.layout_trace):
        parser.error("layout samples require --layout-trace and must be between 0 and 30")
    if args.cached_geometry_check and not args.layout_samples:
        parser.error("--cached-geometry-check requires positive --layout-samples and --layout-trace")
    if args.edit_within and not args.edit_check:
        parser.error("--edit-within requires --edit-check")
    if args.task_toggle_check and (not args.select or args.table_resize_check
                                   or args.edit_check or args.selection_keys or args.perf_seconds):
        parser.error("--task-toggle-check requires a separate --select click without editing or performance checks")
    if args.table_resize_check and (args.fixture != "82-authored-table-widths.md"
                                   or not args.select or args.edit_check or args.selection_keys
                                   or args.copy_selected is not None):
        parser.error("table resize requires fixture 82 and --select, without selection/edit oracles")
    if args.edit_expect is not None and not args.edit_check:
        parser.error("--edit-expect requires --edit-check")
    if args.edit_preserve and not args.edit_check:
        parser.error("--edit-preserve requires --edit-check")
    if args.copy_selected is not None and (not args.select or args.copy_order_check):
        parser.error("--copy-selected requires --select and excludes --copy-order-check")
    if args.copy_expect_file is not None and not args.copy_order_check:
        parser.error("--copy-expect-file requires --copy-order-check")
    if args.selection_keys and (not args.select or len(args.selection_keys) > 40):
        parser.error("selection keys require --select and at most 40 actions")
    if not 0 <= args.edit_idle_seconds <= 5 or (args.edit_idle_seconds and not args.edit_check):
        parser.error("edit idle capture requires --edit-check and 0–5 seconds")
    if args.edit_zoom_steps and not args.edit_check:
        parser.error("edit zoom requires --edit-check")
    if args.edit_paste is not None and not (args.edit_check or args.convert_html_check):
        parser.error("edit paste requires --edit-check or --convert-html-check")
    if args.edit_markdown and (not args.edit_check or args.edit_paste is None):
        parser.error("Markdown paste requires --edit-check and --edit-paste")
    if args.edit_paste is not None and args.html_compose_check:
        parser.error("native paste and dead-key composition are separate checks")
    if args.edit_blur and not (args.edit_check or args.convert_html_check):
        parser.error("edit blur requires --edit-check or --convert-html-check")
    if args.edit_paste is not None and not 0 < len(args.edit_paste.encode()) <= 4096:
        parser.error("edit paste must contain 1–4096 UTF-8 bytes")
    if args.edit_compose_check and (not args.edit_check or args.edit_paste is not None or args.convert_html_check or args.perf_seconds):
        parser.error("edit composition requires --edit-check separately from paste, HTML conversion and performance")
    if args.edit_compose_cancel_check and (not args.edit_compose_check or not args.copy_selected):
        parser.error("composition cancellation requires --edit-compose-check and nonempty --copy-selected")
    if not -3 <= args.edit_zoom_steps <= 3:
        parser.error("edit zoom steps must be between -3 and 3")
    if not 0 < args.startup_wait <= 30:
        parser.error("startup wait must be between 0 and 30 seconds")
    if not 0 <= args.select_hold_seconds <= 10 or (args.select_hold_seconds and not args.select):
        parser.error("selection hold requires --select and 0–10 seconds")
    if not 0 <= args.reflow_wait <= 30:
        parser.error("reflow wait must be between 0 and 30 seconds")
    if args.convert_html_check and args.fixture not in ("10-html-fragments.md", "23-disclosure-editing.md", "29-multilingual-html.md", "30-html-images.md"):
        parser.error("HTML conversion checks use synthetic fixtures 10, 23, 29 or 30")
    if args.html_images_check and args.fixture != "30-html-images.md":
        parser.error("HTML image checks use fixture 30")
    if args.convert_html_check and args.fixture in ("23-disclosure-editing.md", "29-multilingual-html.md") and not args.html_edit_within:
        parser.error("Disclosure/multilingual conversion checks require an explicit --html-edit-within target")
    if args.html_link_check and (args.fixture != "10-html-fragments.md" or args.convert_html_check):
        parser.error("HTML link check requires fixture 10 and a separate invocation from conversion")
    if args.html_clipboard_check and (args.fixture not in ("44-cross-preview-selection.md", "128-html-clipboard.md") or args.edit_check or args.convert_html_check or args.perf_seconds):
        parser.error("HTML clipboard check requires fixture 44/128 separately from edit/performance checks")
    if args.disclosure_check and args.fixture not in ("20-disclosures.md", "22-implicit-disclosures.md", "36-rich-cell-panels.md"):
        parser.error("Disclosure checks use synthetic fixtures 20, 22 or 36")
    if args.html_anchor_check and args.fixture != "26-html-anchors.md":
        parser.error("HTML anchor checks use synthetic fixture 26")
    if args.html_anchor_keyboard and not args.html_anchor_check:
        parser.error("--html-anchor-keyboard requires --html-anchor-check")
    if (args.html_edit_here or args.html_edit_within or args.html_direct_edit) and not args.convert_html_check:
        parser.error("HTML targeted editing requires --convert-html-check")
    if args.html_direct_edit and (args.html_edit_here or not args.html_edit_within):
        parser.error("direct HTML editing requires --html-edit-within and excludes --html-edit-here")
    if args.html_compose_check and not args.html_direct_edit:
        parser.error("--html-compose-check requires --html-direct-edit")
    if not 1 <= args.scroll_steps <= 200:
        parser.error("--scroll-steps must be between 1 and 200")
    if not 0 <= args.scroll_settle_seconds <= 30:
        parser.error("--scroll-settle-seconds must be between 0 and 30")
    if not -3 <= args.zoom_steps <= 10:
        parser.error("--zoom-steps must be between -3 and 10")
    if args.select_before_zoom and not args.zoom_steps:
        parser.error("--select-before-zoom requires nonzero --zoom-steps")
    if args.layout_probes and (len(args.layout_probes) % 2 or not (args.select or args.edit_check)):
        parser.error("layout probes need coordinate pairs and a selection or edit check")
    if args.probe_color and (not args.layout_probes or any(not 0 <= c <= 255 for c in args.probe_color)):
        parser.error("probe color needs layout probes and RGB values in 0..255")
    if args.perf_sweep_seconds and (args.perf_sweep_seconds < 1 or not args.perf_seconds
                                   or args.perf_input != "continuous" or not args.atspi_active):
        parser.error("--perf-sweep-seconds requires performance mode, continuous input and active AT")
    if args.perf_warm_sweep and not args.perf_sweep_seconds:
        parser.error("--perf-warm-sweep requires --perf-sweep-seconds")
    if args.atspi_activation_sample and (
            not args.atspi_active or not args.generated_bytes or args.perf_sweep_seconds):
        parser.error("--atspi-activation-sample requires generated bytes and active AT without a full sweep")
    if args.a07_geometry_check and (
            not args.atspi_active or not args.layout_trace or not args.source_unchanged_check
            or args.fixture != "41-find-document.md" or args.source_document
            or args.zoom_steps or args.perf_seconds):
        parser.error("--a07-geometry-check requires fixture41, active AT, layout trace, unchanged-source check and 100% text")
    if args.perf_seconds < 0 or args.generated_bytes < 0 or (args.coast_check and args.perf_seconds):
        parser.error("use nonnegative performance options, separately from --coast-check")
    if args.atspi_html_check and (args.fixture != "28-accessible-html.md" or args.atspi_check or args.perf_seconds):
        parser.error("--atspi-html-check uses fixture 28 separately from other AT/performance checks")
    args.output = args.output.resolve()
    args.binary = args.binary.resolve()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    if args.appearance_cycle and not args.appearance:
        parser.error("appearance cycle requires an initial --appearance")
    if args.resize_axis != "width" and not args.resize_check:
        parser.error("resize axis requires --resize-check")
    if args.resize_target.startswith("specification-") and (not args.resize_check or args.fixture != "03-technical-reference.md"
                                            or args.zoom_steps not in (0, 5, 10)):
        parser.error("specification resize targets require fixture03, --resize-check and 100, 150 or 200% text")
    if (args.resize_target.startswith("timeline-") or (args.resize_check and args.fixture == "119-rich-timelines.md")) and (
            not args.resize_check or args.fixture != "119-rich-timelines.md"
            or not args.resize_target.startswith("timeline-") or args.zoom_steps not in (0, 5, 10)):
        parser.error("timeline resize targets require fixture119, an explicit timeline target, --resize-check and 100, 150 or 200% text")
    if args.resize_burst and (not args.resize_check or not args.layout_trace):
        parser.error("--resize-burst requires --resize-check and --layout-trace")
    if args.resize_continue_edit and (args.resize_check != "editing" or args.fixture != "122-paired-records.md"
                                     or not args.resize_target.startswith("record-")):
        parser.error("--resize-continue-edit requires fixture122, editing and an explicit record target")
    if (args.resize_target.startswith("record-") or (args.resize_check and args.fixture == "122-paired-records.md")) and (
            not args.resize_check or args.fixture != "122-paired-records.md"
            or not args.resize_target.startswith("record-") or args.zoom_steps not in (0, 5, 10)):
        parser.error("record resize targets require fixture122, an explicit record target, --resize-check and 100, 150 or 200% text")
    if args.atspi_gallery_check and (args.fixture != "32-accessible-gallery.md" or args.atspi_check or args.atspi_html_check or args.perf_seconds):
        parser.error("gallery accessibility checks require fixture 32 and no other accessibility/performance check")
    if args.atspi_editorial_check and (
            args.fixture != "47-editorial-composition.md"
            or args.atspi_check or args.atspi_html_check or args.atspi_gallery_check
            or args.atspi_math_check or args.atspi_active or args.perf_seconds or args.edit_check):
        parser.error("editorial accessibility requires fixture 47 and no other accessibility/edit/performance check")
    if args.atspi_check and args.fixture != "27-accessible-document.md":
        parser.error("--atspi-check uses the bounded synthetic fixture 27")
    if args.atspi_math_check and (args.fixture != "40-accessible-math.md" or args.atspi_check or args.atspi_html_check or args.atspi_gallery_check or args.perf_seconds):
        parser.error("math accessibility checks require fixture 40 and no other accessibility/performance check")
    with tempfile.TemporaryDirectory(prefix="mineral-layout-") as directory, ExitStack() as resources:
        work = Path(directory)
        runtime = work / "runtime"
        runtime.mkdir(mode=0o700)
        env = dict(os.environ, XDG_RUNTIME_DIR=str(runtime), WAYLAND_DISPLAY="layout",
                   XDG_STATE_HOME=str(work / "state"), XDG_CACHE_HOME=str(work / "cache"),
                   MINERAL_WESTON_SCALE_120=str(args.scale))
        env.pop("MINERAL_INSTANCE_MODE", None)
        env.pop("MINERAL_INSTANCE_SOCKET", None)
        env.pop("MINERAL_LAYOUT_TRACE", None)
        from accessibility_probe import private_bus
        env = resources.enter_context(private_bus(env, enabled=args.html_clipboard_check or args.recovery_controls_check or args.unsaved_close_check or args.window_controls_check or args.titlebar_check or args.appearance_cycle or args.atspi_check or args.atspi_active or args.atspi_html_check or args.atspi_gallery_check or args.atspi_editorial_check or args.atspi_math_check or args.find_check or args.layout_recovery_check or args.resize_check or args.boundary_check or args.font_reflow_check or args.rtl_check or args.adaptive_check or args.chapter_identity_check or args.html_effects_check or bool(args.edit_compose_check) or bool(args.recorded_bugs_check)))
        if args.appearance:
            from theme_portal import settings_portal
            resources.enter_context(settings_portal(env, args.appearance))
        if args.layout_trace or args.atspi_activation_sample:
            env["MINERAL_LAYOUT_TRACE"] = args.layout_trace or "summary"
        # Fixture edits and autosave are confined to this copy.
        shutil.copytree(ROOT / "performance/layout-fixtures", work / "layout-fixtures")
        shutil.copytree(ROOT / "performance/visual-assets", work / "visual-assets")
        source_origin = args.source_document.resolve(strict=True) if args.source_document else None
        if source_origin:
            args.fixture = stage_source_document(source_origin, work / "layout-fixtures")
        resource_evidence = stage_source_resources(
            source_origin, work / "layout-fixtures" / args.fixture, work,
            args.source_resource_dir) if source_origin else []
        if args.generated_bytes:
            args.fixture = "generated-scroll.md"
            subprocess.run([str(ROOT / "target/release/mineral-fixture"), "--bytes", str(args.generated_bytes),
                            "--output", str(work / "layout-fixtures" / args.fixture)], check=True)
        view_source_path = work / "layout-fixtures" / args.fixture
        effects_monitor = None
        if args.html_effects_check:
            from html_effects_check import inert_effect_server
            effects_monitor = resources.enter_context(inert_effect_server(view_source_path))
        view_original_source = view_source_path.read_bytes() if args.source_unchanged_check else None
        fixture_bytes = view_source_path.stat().st_size
        if args.perf_seconds:
            # A large initial AT-SPI publication is intentionally measured as
            # readiness evidence, separately from steady-state scrolling. Give
            # that publication enough room to finish before the timed window.
            activation_warmup = (60 if args.atspi_activation_sample
                                 and fixture_bytes >= 8 * 1024 * 1024 else
                                 20 if args.atspi_activation_sample else
                                 10 if args.atspi_active else 0)
            env.update(MINERAL_PERF_OUTPUT=str(work / "perf.json"), MINERAL_PERF_SECONDS=str(args.perf_seconds),
                       MINERAL_PERF_WARMUP_MS=str(round((max(5, args.startup_wait + 2 +
                                                       activation_warmup)
                                                       + (2 * args.perf_sweep_seconds + 2 if args.perf_warm_sweep else 0)) * 1000)),
                       MINERAL_PERF_REFRESH_HZ="120", MINERAL_PERF_RESIZE="false",
                       MINERAL_PERF_LABEL="isolated-wayland-momentum", MINERAL_PERF_SCENARIO=f"bidirectional {args.perf_input} scrolling",
                       MINERAL_PERF_INPUT_SOURCE="private Wayland seat on isolated Weston 14 headless GL output")
        modules = ROOT / "performance/wayland-harness/build"
        with (work / "weston.log").open("w") as log:
            weston = subprocess.Popen([
                "weston", "--backend=headless", "--renderer=gl", f"--width={args.width}",
                f"--height={args.height}", "--socket=layout", "--idle-time=0", "--debug",
                f"--config={ROOT / 'performance/wayland-harness/compose.ini'}" if args.html_compose_check or args.edit_compose_check else "--no-config",
                f"--shell={'desktop' if args.window_controls_check or args.resize_check or args.boundary_check or args.font_reflow_check else 'kiosk'}", f"--refresh-rate={args.refresh_rate}",
                f"--modules={modules / 'virtual-input.so'},{modules / 'fractional-scale.so'}",
            ], env=env, stdout=log, stderr=log)
            app = None
            binary_hash = None
            try:
                deadline = time.monotonic() + 10
                while not (runtime / "layout").exists():
                    if weston.poll() is not None or time.monotonic() > deadline:
                        raise RuntimeError((work / "weston.log").read_text())
                    time.sleep(0.05)
                if args.idle_layout_check:
                    subprocess.run([str(modules / "input-client"), "move", "800", "350"], env=env, check=True)
                if args.recovery_controls_check:
                    from status_controls_check import seed_recovery
                    seed_recovery(work / "layout-fixtures" / args.fixture, work)
                app_started_at = time.monotonic()
                app = subprocess.Popen([str(args.binary),
                                        str(work / "layout-fixtures" / args.fixture)],
                                       env=env, stdout=log, stderr=log)
                # Identify the running executable, even if Cargo replaces its
                # pathname while a diagnostic capture is in progress.
                binary_hash = hashlib.sha256(Path(f"/proc/{app.pid}/exe").read_bytes()).hexdigest()
                time.sleep(args.startup_wait)
                if app.poll() is not None:
                    raise RuntimeError((work / "weston.log").read_text())
                # Weston 14's refresh=0 deliberately repaints only on capture.
                # One capture presents current buffers and releases callbacks;
                # asynchronous font/image/layout results need subsequent frames.
                # Keep this explicit so it cannot masquerade as idle readiness.
                for index in range(args.startup_capture_frames):
                    capture = work / f"startup-frame-{index}"
                    capture.mkdir()
                    subprocess.run(["weston-screenshooter"], cwd=capture, env=env, check=True, timeout=10)
                    time.sleep(0.1)
                def input_event(*values):
                    subprocess.run([str(modules / "input-client"), *map(str, values)], env=env, check=True)
                def verify_source_unchanged():
                    if not args.source_unchanged_check:
                        return
                    if app.poll() is None:
                        time.sleep(1.2)  # allow an accidental content event to reach autosave
                    current_source = view_source_path.read_bytes()
                    unchanged = current_source == view_original_source
                    result = dict(source_unchanged=unchanged, passes=unchanged,
                                  fixture=args.fixture, binary_sha256=binary_hash,
                                  original_sha256=hashlib.sha256(view_original_source).hexdigest(),
                                  current_sha256=hashlib.sha256(current_source).hexdigest(),
                                  width=args.width, height=args.height, zoom_steps=args.zoom_steps)
                    if source_origin:
                        result["source_origin"] = source_origin_evidence(source_origin, view_original_source)
                        result["source_resources"] = resource_evidence
                    args.output.with_suffix(".source.json").write_text(json.dumps(result, indent=2) + "\n")
                    print(json.dumps(result), flush=True)
                    if not unchanged:
                        raise RuntimeError("View interaction changed the fixture's source bytes")
                def planning_reports():
                    log.flush()
                    prefix = "MINERAL_LAYOUT_TRACE "
                    result = []
                    for line in (work / "weston.log").read_text().splitlines():
                        if line.startswith(prefix):
                            try:
                                result.append(json.loads(line[len(prefix):]))
                            except json.JSONDecodeError:
                                pass  # the worker may still be writing this line
                    return result
                if args.startup_ready_width is not None:
                    deadline = time.monotonic() + 3
                    while True:
                        reports = planning_reports()
                        ready = measured_startup_ready(reports, args.startup_ready_width)
                        if ready or app.poll() is not None or time.monotonic() >= deadline:
                            break
                        time.sleep(0.02)
                    result = dict(passes=ready, binary_sha256=binary_hash, fixture=args.fixture,
                                  fixture_sha256=hashlib.sha256(view_source_path.read_bytes()).hexdigest(),
                                  refresh_rate_mhz=args.refresh_rate, expected_canvas_width=args.startup_ready_width,
                                  committed=sum(bool(r.get("committed")) for r in reports),
                                  startup_capture_frames=args.startup_capture_frames,
                                  readiness_mode="capture_driven" if args.startup_capture_frames else "idle",
                                  native_input_sent=args.idle_layout_check,
                                  post_launch_input_sent=False)
                    args.output.with_suffix(".startup.json").write_text(json.dumps(result, indent=2) + "\n")
                    print(json.dumps(result), flush=True)
                    if not ready:
                        raise RuntimeError("Startup did not publish the measured viewport with resolved images")
                if args.idle_layout_check:
                    from PIL import Image
                    if args.fixture not in ("57-dated-timelines.md", "63-semantic-range-editing.md") or not args.layout_trace or args.width != 1600 or args.height != 1200:
                        raise RuntimeError("Idle layout probe requires fixture 57/63, 1600x1200 and layout trace")
                    deadline = time.monotonic() + 10
                    while not any(r.get("committed") and r.get("canvas_width", 0) > 1000 for r in planning_reports()):
                        if app.poll() is not None or time.monotonic() > deadline:
                            raise RuntimeError("No committed startup layout to check")
                        time.sleep(0.02)
                    frames = []
                    for stage in ["idle", "hover"]:
                        if stage == "hover":
                            input_event("move", 100, 100)  # sidebar; no document state change
                        time.sleep(0.2)
                        capture = work / f"startup-{stage}"
                        capture.mkdir()
                        subprocess.run(["weston-screenshooter"], cwd=capture, env=env, check=True, timeout=10)
                        screenshot, = capture.glob("*.png")
                        shutil.copyfile(screenshot, args.output.with_name(f"{args.output.stem}-{stage}.png"))
                        with Image.open(screenshot) as frame:
                            # Excludes sidebar hover, the title caret and the scrollbar.
                            rgb = frame.convert("RGB")
                            frames.append(rgb if args.fixture == "63-semantic-range-editing.md"
                                          else rgb.crop((400, 260, 1550, 1140)))
                    # Ignore the initial pointer sprite at (800,350), not the
                    # surrounding document; sidebar hover is outside the crop.
                    result = (check_idle_definition_frames(frames) if args.fixture == "63-semantic-range-editing.md"
                              else check_idle_document_frames(frames))
                    result.update(binary_sha256=binary_hash, fixture=args.fixture,
                                  refresh_rate_mhz=args.refresh_rate)
                    args.output.with_suffix(".idle.json").write_text(json.dumps(result, indent=2) + "\n")
                    print(json.dumps(result), flush=True)
                    if not result["passes"]:
                        raise RuntimeError("Committed startup layout was invisible until pointer input")
                if args.zoom_steps:
                    if args.select_before_zoom:
                        input_event("key", 29, 1)
                        input_event("key", 30, 1)  # Ctrl+A
                        input_event("key", 30, 0)
                        input_event("key", 29, 0)
                    input_event("key", 29, 1)  # Linux KEY_LEFTCTRL
                    for _ in range(abs(args.zoom_steps)):
                        key = 13 if args.zoom_steps > 0 else 12  # KEY_EQUAL / KEY_MINUS
                        input_event("key", key, 1)
                        input_event("key", key, 0)
                        time.sleep(0.05)
                    input_event("key", 29, 0)
                    time.sleep(args.reflow_wait)
                for sample in range(args.layout_samples):
                    before = sum(report.get("explicitly_requested", False) for report in planning_reports())
                    for key in [29, 56, 42, 38]:  # Ctrl+Alt+Shift+L
                        input_event("key", key, 1)
                    for key in [38, 42, 56, 29]:
                        input_event("key", key, 0)
                    deadline = time.monotonic() + 30
                    while sum(report.get("explicitly_requested", False) for report in planning_reports()) <= before:
                        if app.poll() is not None or time.monotonic() >= deadline:
                            raise RuntimeError(f"Requested layout sample {sample + 1} did not complete")
                        time.sleep(0.02)
                    time.sleep(0.1)
                if args.scroll:
                    subprocess.run([str(modules / "input-client"), "move", str(args.width // 2),
                                    str(args.height // 2)], env=env, check=True)
                    for _ in range(args.scroll_steps):
                        subprocess.run([str(modules / "input-client"), "scroll", "0", str(args.scroll)],
                                       env=env, check=True)
                        time.sleep(0.04)
                    time.sleep(args.scroll_settle_seconds)
                if args.horizontal_scroll:
                    x, y, delta = args.horizontal_scroll
                    input_event("move", x, y)
                    for _ in range(args.scroll_steps):
                        input_event("scroll", delta, 0)
                        time.sleep(0.04)
                    time.sleep(0.3)
                accessibility_active = False
                accessible_nodes = 0
                accessibility_sample = None
                accessibility_readiness_seconds = None
                if args.recorded_bugs_check:
                    if args.recorded_bugs_check == "table-history":
                        from table_history_check import check
                    else:
                        from recorded_bugs_check import check
                    result = check(args.recorded_bugs_check, env, input_event, view_source_path,
                                   app.pid, args.output, ROOT / "performance/accessibility_probe.py",
                                   work, args.width, 1 + args.zoom_steps / 10)
                    result.update(binary_sha256=binary_hash, width=args.width, height=args.height,
                                  display_scale_120=args.scale)
                    args.output.with_suffix(".bugs.json").write_text(json.dumps(result, indent=2) + "\n")
                    print(f"Native recorded bug check: {args.recorded_bugs_check} passed", flush=True)
                if args.layout_recovery_check:
                    from layout_recovery_check import check
                    def validation_events():
                        log.flush()
                        prefix = "MINERAL_LAYOUT_VALIDATION "
                        return [line[len(prefix):] for line in (work / "weston.log").read_text().splitlines() if line.startswith(prefix)]
                    result = check(args.layout_recovery_check, env, input_event,
                                   work / "layout-fixtures" / args.fixture, app.pid, args.output,
                                   ROOT / "performance/accessibility_probe.py", work, validation_events)
                    result.update(binary_sha256=binary_hash, width=args.width, height=args.height)
                    args.output.with_suffix(".recovery.json").write_text(json.dumps(result, indent=2) + "\n")
                    print("Native layout recovery, source order, edit and undo: pass", flush=True)
                if args.resize_check:
                    from resize_layout_check import check
                    result = check(args.resize_check, env, input_event,
                                   work / "layout-fixtures" / args.fixture, app.pid, args.output,
                                   ROOT / "performance/accessibility_probe.py", work,
                                   work / "weston.log", scale=args.scale, axis=args.resize_axis,
                                   burst=args.resize_burst, target=args.resize_target, zoom=1 + args.zoom_steps / 10,
                                   continue_edit=args.resize_continue_edit)
                    result.update(binary_sha256=binary_hash, width=args.width, height=args.height,
                                  display_scale_120=args.scale, private_session_bus=True)
                    args.output.with_suffix(".resize.json").write_text(
                        json.dumps(result, indent=2) + "\n"
                    )
                    print(f"Native {args.resize_check} resize stability: pass={result['passes']}",
                          flush=True)
                    if not result["passes"]:
                        raise RuntimeError("Native automatic-layout resize stability check failed")
                if args.boundary_check:
                    from boundary_layout_check import check
                    result = check(
                        env, input_event, work / "layout-fixtures" / args.fixture,
                        app.pid, args.output, ROOT / "performance/accessibility_probe.py", work,
                    )
                    result.update(
                        binary_sha256=binary_hash, width=args.width, height=args.height,
                        display_scale_120=args.scale, private_session_bus=True,
                        layout_intent="automatic-only",
                    )
                    args.output.with_suffix(".boundary.json").write_text(
                        json.dumps(result, indent=2) + "\n"
                    )
                    print(
                        "Native automatic-only boundary stability: "
                        f"pass={result['passes']} transition={result['transition']}",
                        flush=True,
                    )
                    if not result["passes"]:
                        raise RuntimeError("Native automatic-only boundary stability check failed")
                if args.font_reflow_check:
                    from font_reflow_check import check
                    result = check(
                        env, input_event, work / "layout-fixtures" / args.fixture,
                        app.pid, args.output, ROOT / "performance/accessibility_probe.py",
                        work, work / "weston.log",
                    )
                    result.update(
                        binary_sha256=binary_hash, width=args.width, height=args.height,
                        display_scale_120=args.scale, zoom_steps=args.zoom_steps,
                        private_session_bus=True,
                    )
                    args.output.with_suffix(".font.json").write_text(
                        json.dumps(result, indent=2) + "\n"
                    )
                    print(
                        f"Native body-font remeasurement at zoom {args.zoom_steps}: "
                        f"pass={result['passes']}", flush=True,
                    )
                    if not result["passes"]:
                        raise RuntimeError("Native body-font remeasurement check failed")
                if args.rtl_check:
                    from rtl_direction_check import check
                    result = check(
                        env, input_event, work / "layout-fixtures" / args.fixture,
                        app.pid, ROOT / "performance/accessibility_probe.py",
                        work / "weston.log",
                    )
                    result.update(
                        binary_sha256=binary_hash, width=args.width, height=args.height,
                        display_scale_120=args.scale, zoom_steps=args.zoom_steps,
                        private_session_bus=True,
                    )
                    args.output.with_suffix(".rtl.json").write_text(
                        json.dumps(result, indent=2) + "\n"
                    )
                    print(f"Native RTL caret and hit testing: pass={result['passes']}", flush=True)
                    if not result["passes"]:
                        raise RuntimeError("Native RTL caret/hit-test check failed")
                if args.adaptive_check:
                    from adaptive_layout_check import check
                    result = check(
                        args.adaptive_check, args.adaptive_profile, env,
                        work / "layout-fixtures" / args.fixture, app.pid, args.output,
                        ROOT / "performance/accessibility_probe.py", work,
                    )
                    result.update(
                        binary_sha256=binary_hash, width=args.width, height=args.height,
                        display_scale_120=args.scale, zoom_steps=args.zoom_steps,
                        private_session_bus=True,
                    )
                    args.output.with_suffix(".adaptive.json").write_text(
                        json.dumps(result, indent=2) + "\n"
                    )
                    print(
                        f"Native adaptive {args.adaptive_check} at {args.adaptive_profile}: "
                        f"pass={result['passes']}", flush=True,
                    )
                    if not result["passes"]:
                        raise RuntimeError("Native adaptive-layout check failed")
                if args.chapter_identity_check:
                    from chapter_identity_check import check
                    result = check(
                        env, input_event, work / "layout-fixtures" / args.fixture,
                        app.pid, args.output, ROOT / "performance/accessibility_probe.py", work,
                    )
                    result.update(
                        binary_sha256=binary_hash, width=args.width, height=args.height,
                        display_scale_120=args.scale, zoom_steps=args.zoom_steps,
                        private_session_bus=True,
                    )
                    args.output.with_suffix(".identity.json").write_text(
                        json.dumps(result, indent=2) + "\n"
                    )
                    print(
                        f"Native chapter barriers and duplicate identities: pass={result['passes']}",
                        flush=True,
                    )
                    if not result["passes"]:
                        raise RuntimeError("Native chapter identity check failed")
                if args.html_effects_check:
                    from html_effects_check import check
                    result = check(
                        env, work / "layout-fixtures" / args.fixture, app.pid,
                        args.output, ROOT / "performance/accessibility_probe.py", work,
                        effects_monitor,
                    )
                    result.update(
                        binary_sha256=binary_hash, width=args.width, height=args.height,
                        display_scale_120=args.scale, zoom_steps=args.zoom_steps,
                        layout_samples=args.layout_samples, private_session_bus=True,
                    )
                    args.output.with_suffix(".effects.json").write_text(
                        json.dumps(result, indent=2) + "\n"
                    )
                    print(
                        f"Native inert HTML effects: pass={result['passes']}", flush=True,
                    )
                    if not result["passes"]:
                        raise RuntimeError("Native inert HTML effects check failed")
                titlebar_states = {}
                if args.titlebar_check:
                    from titlebar_check import inspect
                    titlebar_states = inspect(env, input_event, work / "layout-fixtures" / args.fixture,
                                              app.pid, args.output, ROOT / "performance/accessibility_probe.py", work, args.scale)
                if args.find_check:
                    if args.fixture not in ("41-find-document.md", "42-find-overflow.md") or args.perf_seconds or args.edit_check:
                        raise RuntimeError("Find check requires fixture 41/42 separately from editing/performance checks")
                    from find_check import check
                    result = check(env, input_event, work / "layout-fixtures" / args.fixture,
                                   app.pid, args.output, ROOT / "performance/accessibility_probe.py", work,
                                   (args.width, args.height))
                    result.update(binary_sha256=binary_hash, width=args.width, height=args.height, zoom_steps=args.zoom_steps)
                    args.output.with_suffix(".find.json").write_text(json.dumps(result, indent=2) + "\n")
                    print("Native find, disclosure reveal, copy, edit and undo: pass", flush=True)
                if args.atspi_math_check:
                    from math_accessibility_check import check
                    source_path = work / "layout-fixtures" / args.fixture
                    original = source_path.read_bytes()
                    probe = subprocess.run([
                        "/usr/bin/python3", str(ROOT / "performance/accessibility_probe.py"), str(app.pid),
                        "--exercise-math-scroll",
                    ], env=env, capture_output=True, text=True, timeout=30)
                    if probe.returncode:
                        raise RuntimeError(probe.stderr)
                    probe_result = json.loads(probe.stdout)
                    nodes = probe_result["nodes"]
                    result = check(nodes)
                    if source_path.read_bytes() != original:
                        raise RuntimeError("Math accessibility changed authored source")
                    result.update(binary_sha256=binary_hash, source_unchanged=True, nodes=nodes,
                                  math_scroll=probe_result["math_scroll"],
                                  width=args.width, height=args.height, zoom_steps=args.zoom_steps)
                    args.output.with_suffix(".math-atspi.json").write_text(json.dumps(result, indent=2) + "\n")
                    print("Native math structure, token order and unchanged source: pass", flush=True)
                if args.atspi_gallery_check:
                    source_path = work / "layout-fixtures" / args.fixture
                    original = source_path.read_bytes()
                    probe = subprocess.run([
                        "/usr/bin/python3", str(ROOT / "performance/accessibility_probe.py"),
                        str(app.pid), "--exercise-gallery",
                    ], env=env, capture_output=True, text=True, timeout=30)
                    if probe.returncode:
                        raise RuntimeError(probe.stderr)
                    result = json.loads(probe.stdout)
                    result.update(binary_sha256=binary_hash, fixture=args.fixture,
                                  width=args.width, height=args.height, display_scale_120=args.scale,
                                  zoom_steps=args.zoom_steps, private_session_bus=True,
                                  source_unchanged=source_path.read_bytes() == original)
                    result["passes"] = (result["source_unchanged"] and result["gallery"]["passes"]
                                        and result["gallery"]["identities_retained"])
                    args.output.with_suffix(".atspi.json").write_text(json.dumps(result, indent=2) + "\n")
                    if not result["passes"]:
                        raise RuntimeError("Native gallery accessibility/source check failed")
                    print("Native gallery image order, link activation and unchanged source: pass", flush=True)
                if args.atspi_editorial_check:
                    from editorial_accessibility_check import check
                    source_path = work / "layout-fixtures" / args.fixture
                    original = source_path.read_bytes()
                    probe = subprocess.run([
                        "/usr/bin/python3", str(ROOT / "performance/accessibility_probe.py"), str(app.pid),
                    ], env=env, capture_output=True, text=True, timeout=30)
                    if probe.returncode:
                        raise RuntimeError(probe.stderr)
                    nodes = json.loads(probe.stdout)["nodes"]
                    result = check(nodes)
                    result.update(
                        binary_sha256=binary_hash,
                        fixture=args.fixture,
                        width=args.width,
                        height=args.height,
                        display_scale_120=args.scale,
                        zoom_steps=args.zoom_steps,
                        private_session_bus=True,
                        source_unchanged=source_path.read_bytes() == original,
                        nodes=nodes,
                    )
                    result["passes"] = result["passes"] and result["source_unchanged"]
                    args.output.with_suffix(".atspi.json").write_text(json.dumps(result, indent=2) + "\n")
                    if not result["passes"]:
                        raise RuntimeError("Editorial accessibility/source check failed")
                    print("Native editorial traversal, focus, peer geometry and unchanged source: pass", flush=True)
                if args.atspi_html_check:
                    from html_accessibility_check import check_html_accessibility
                    result = check_html_accessibility(app.pid, env, work / "layout-fixtures" / args.fixture)
                    result.update(binary_sha256=binary_hash, fixture=args.fixture,
                                  width=args.width, height=args.height, display_scale_120=args.scale,
                                  zoom_steps=args.zoom_steps, private_session_bus=True)
                    args.output.with_suffix(".atspi.json").write_text(json.dumps(result, indent=2) + "\n")
                    if not result["passes"]:
                        raise RuntimeError("HTML accessibility content/disclosure/source check failed")
                    print("Native HTML accessibility, disclosure open/close and unchanged source: pass", flush=True)
                if args.atspi_active and not args.atspi_check:
                    probe_arguments = [
                        "/usr/bin/python3", str(ROOT / "performance/accessibility_probe.py"),
                        str(app.pid),
                    ]
                    if args.atspi_activation_sample:
                        expected_roots = generated_accessible_root_count(view_source_path.read_bytes())
                        probe_arguments.extend([
                            "--activation-sample",
                            f"--expected-document-children={expected_roots}",
                        ])
                        layout_deadline = time.monotonic() + 120
                        while True:
                            reports = planning_reports()
                            generated_layout_ready = any(
                                report.get("committed")
                                and report.get("original_source_bytes") == fixture_bytes
                                and report.get("root_count") == expected_roots
                                and report.get("unresolved_image_dimensions") == 0
                                for report in reports
                            )
                            if (generated_layout_ready or app.poll() is not None
                                    or time.monotonic() >= layout_deadline):
                                break
                            time.sleep(.1)
                        if not generated_layout_ready:
                            raise RuntimeError(
                                "Generated layout did not become ready before AT-SPI activation"
                            )
                    activation_deadline = time.monotonic() + 60
                    last_probe_error = None
                    while True:
                        try:
                            active_probe = subprocess.run(
                                probe_arguments, env=env, capture_output=True, text=True,
                                timeout=15,
                            )
                            if active_probe.returncode:
                                last_probe_error = active_probe.stderr.strip()
                                active_result = None
                            else:
                                active_result = json.loads(active_probe.stdout)
                                last_probe_error = None
                        except (subprocess.TimeoutExpired, json.JSONDecodeError) as error:
                            last_probe_error = str(error)
                            active_result = None
                        if (active_result is not None
                                and (not args.atspi_activation_sample or active_result["passes"])):
                            break
                        if time.monotonic() >= activation_deadline or app.poll() is not None:
                            if last_probe_error:
                                raise RuntimeError(
                                    "AT-SPI activation did not stabilize: " + last_probe_error
                                )
                            break
                        time.sleep(.25)
                    accessibility_active = True
                    accessibility_readiness_seconds = time.monotonic() - app_started_at
                    if args.atspi_activation_sample:
                        accessibility_sample = active_result
                        args.output.with_suffix(".active-atspi.json").write_text(json.dumps(dict(
                            binary_sha256=binary_hash, fixture=args.fixture,
                            width=args.width, height=args.height, zoom_steps=args.zoom_steps,
                            private_session_bus=True,
                            accessibility_readiness_seconds=accessibility_readiness_seconds,
                            **accessibility_sample), indent=2) + "\n")
                        if not accessibility_sample["passes"]:
                            raise RuntimeError("Bounded native accessibility sample failed")
                        print(
                            "Private AT-SPI active: "
                            f"{accessibility_sample['document_child_count']} document roots; "
                            f"{accessibility_sample['sampled_direct_children']} sampled",
                            flush=True,
                        )
                    else:
                        active_tree = active_result["nodes"]
                        accessible_nodes = len(active_tree)
                        if not any(node["role"] == "document frame" for node in active_tree):
                            raise RuntimeError("Native document accessibility was not active for this measurement")
                        if not args.perf_seconds or args.perf_sweep_seconds:
                            args.output.with_suffix(".active-atspi.json").write_text(json.dumps(dict(
                                binary_sha256=binary_hash, fixture=args.fixture,
                                width=args.width, height=args.height, zoom_steps=args.zoom_steps,
                                private_session_bus=True, nodes=active_tree), indent=2) + "\n")
                        print(f"Private AT-SPI active: {accessible_nodes} nodes", flush=True)
                    if args.a07_geometry_check:
                        from a07_geometry_check import check
                        result = check(
                            active_tree, env, input_event, view_source_path,
                            view_original_source, app.pid,
                            ROOT / "performance/accessibility_probe.py", planning_reports,
                            args.width, args.height, args.scale,
                        )
                        result.update(
                            binary_sha256=binary_hash,
                            fixture=args.fixture,
                            private_session_bus=True,
                        )
                        args.output.with_suffix(".a07.json").write_text(
                            json.dumps(result, indent=2) + "\n"
                        )
                        if not result["passes"]:
                            raise RuntimeError("Native A07 geometry matrix case failed")
                        print(
                            "Native A07 scale, geometry, anchor and Files/Outline: pass",
                            flush=True,
                        )
                if args.atspi_check:
                    probe = subprocess.run([
                        "/usr/bin/python3", str(ROOT / "performance/accessibility_probe.py"), str(app.pid),
                    ], env=env, capture_output=True, text=True, timeout=30)
                    if probe.returncode:
                        raise RuntimeError(probe.stderr)
                    result = json.loads(probe.stdout)
                    result.update(binary_sha256=binary_hash, fixture=args.fixture,
                                  width=args.width, height=args.height, private_session_bus=True,
                                  display_scale_120=args.scale, zoom_steps=args.zoom_steps)
                    headings = [node["name"] for node in result["nodes"] if node["role"] == "heading"]
                    result["all_headings_available"] = headings == [
                        "Accessible document", "1. Property comparison", "Connection properties",
                        "Execution options", "2. Task actions",
                        "3. Source-order list", "4. Code and links", "5. HTML disclosure",
                        "6. Formula", "7. Final offscreen heading",
                    ]
                    def children(index):
                        return [(i, node) for i, node in enumerate(result["nodes"]) if node["parent"] == index]
                    tables = []
                    table_records = []
                    for index, node in enumerate(result["nodes"]):
                        if node["role"] != "table":
                            continue
                        table_records.append(node)
                        tables.append([[(cell["role"], cell["name"]) for _, cell in children(row_index)]
                                       for row_index, row in children(index) if row["role"] == "table row"])
                    result["contextual_table_names"] = [node["name"] for node in table_records] == [
                        "Connection properties table", "Execution options table",
                    ]
                    result["table_header_and_cell_hierarchy"] = tables == [
                        [[("column header", "Property"), ("column header", "Value")],
                         [("table cell", "Protocol"), ("table cell", "Local")],
                         [("table cell", "Timeout"), ("table cell", "30 seconds")]],
                        [[("column header", "Option"), ("column header", "Enabled"), ("column header", "Description")],
                         [("table cell", "Verify"), ("table cell", "Yes"), ("table cell", "Keep source content unchanged")],
                         [("table cell", "Preview"), ("table cell", "Yes"), ("table cell", "Show the original content")]],
                    ]
                    first_table, second_table = (node["bounds"] for node in table_records)
                    result["adaptive_tables_side_by_side"] = (
                        abs(first_table["y"] - second_table["y"]) <= 2
                        and first_table["x"] + first_table["width"] <= second_table["x"]
                    )
                    list_items = [node["name"] for node in result["nodes"] if node["role"] == "list item"]
                    result["list_reading_order"] = list_items == [
                        "North: First independent marker.", "South: Second independent marker.",
                        "East: Third independent marker.", "West: Fourth independent marker.",
                        "Above: Fifth independent marker.", "Below: Sixth independent marker.",
                    ]
                    result["formula_source_accessible"] = any(
                        node["role"] == "math" and node["name"] == r"\frac{37}{41}" for node in result["nodes"])
                    args.output.with_suffix(".atspi.json").write_text(json.dumps(result, indent=2) + "\n")
                    print(json.dumps({"atspi_nodes": len(result["nodes"]), "headings": headings,
                                      "all_headings_available": result["all_headings_available"]}), flush=True)
                    if not result["all_headings_available"]:
                        raise RuntimeError("Native accessibility traversal cannot reach all canonical headings")
                    source_path = work / "layout-fixtures" / args.fixture
                    original = source_path.read_bytes()
                    exercised = subprocess.run([
                        "/usr/bin/python3", str(ROOT / "performance/accessibility_probe.py"),
                        str(app.pid), "--exercise",
                    ], env=env, capture_output=True, text=True, timeout=30)
                    if exercised.returncode:
                        raise RuntimeError(exercised.stderr)
                    actions = json.loads(exercised.stdout)
                    result.update({key: value for key, value in actions.items() if key != "nodes"})
                    expected = original.replace(b"- [ ] Unchecked native task", b"- [x] Unchecked native task")
                    deadline = time.monotonic() + 5
                    while source_path.read_bytes() != expected and time.monotonic() < deadline:
                        time.sleep(0.05)
                    result["task_source_exact"] = source_path.read_bytes() == expected
                    input_event("key", 29, 1)  # Ctrl+Z, only the private app's seat
                    input_event("key", 44, 1)
                    input_event("key", 44, 0)
                    input_event("key", 29, 0)
                    deadline = time.monotonic() + 5
                    while source_path.read_bytes() != original and time.monotonic() < deadline:
                        time.sleep(0.05)
                    result["undo_source_exact"] = source_path.read_bytes() == original
                    restored = subprocess.run([
                        "/usr/bin/python3", str(ROOT / "performance/accessibility_probe.py"), str(app.pid),
                    ], env=env, capture_output=True, text=True, timeout=30)
                    if restored.returncode:
                        raise RuntimeError(restored.stderr)
                    restored_nodes = json.loads(restored.stdout)["nodes"]
                    result["undo_task_state_restored"] = any(
                        node["path"] == result["task_action"]["path"] and "checked" not in node["states"]
                        for node in restored_nodes)
                    result["canonical_heading_order_and_ids_retained"] = [
                        (node["path"], node["name"]) for node in result["nodes"] if node["role"] == "heading"
                    ] == [(node["path"], node["name"]) for node in restored_nodes if node["role"] == "heading"]
                    # A fresh AccessKit consumer must receive a full tree, not
                    # deltas against the disconnected consumer's old baseline.
                    for enabled in (False, True):
                        for name in ("IsEnabled", "ScreenReaderEnabled"):
                            subprocess.run([
                                "gdbus", "call", "--session", "--dest", "org.a11y.Bus",
                                "--object-path", "/org/a11y/bus", "--method",
                                "org.freedesktop.DBus.Properties.Set", "org.a11y.Status", name,
                                "<true>" if enabled else "<false>",
                            ], env=env, check=True, capture_output=True, text=True, timeout=5)
                        time.sleep(.5)
                    reconnected = subprocess.run([
                        "/usr/bin/python3", str(ROOT / "performance/accessibility_probe.py"), str(app.pid),
                    ], env=env, capture_output=True, text=True, timeout=30)
                    if reconnected.returncode:
                        raise RuntimeError(reconnected.stderr)
                    reconnected_nodes = json.loads(reconnected.stdout)["nodes"]
                    result["complete_tree_after_reactivation"] = (
                        [(node["path"], node["name"]) for node in reconnected_nodes if node["role"] == "heading"]
                        == [(node["path"], node["name"]) for node in restored_nodes if node["role"] == "heading"]
                        and any(node["path"] == result["task_action"]["path"]
                                and "checked" not in node["states"] for node in reconnected_nodes)
                        and source_path.read_bytes() == original)
                    result["passes"] = all(result[key] for key in (
                        "all_headings_available", "heading_ids_stable_after_scroll", "task_source_exact",
                        "undo_source_exact", "undo_task_state_restored", "canonical_heading_order_and_ids_retained",
                        "contextual_table_names", "table_header_and_cell_hierarchy",
                        "adaptive_tables_side_by_side", "list_reading_order", "formula_source_accessible",
                        "complete_tree_after_reactivation"))
                    args.output.with_suffix(".atspi.json").write_text(json.dumps(result, indent=2) + "\n")
                    print(json.dumps({key: value for key, value in result.items() if key != "nodes"}), flush=True)
                    if not result["passes"]:
                        raise RuntimeError("Native accessibility action, identity, source-order or undo check failed")
                if args.copy_order_check:
                    input_event("move", args.width // 2, min(args.height - 50, 200))
                    input_event("button", 272, 1)
                    input_event("button", 272, 0)
                    input_event("key", 29, 1)
                    input_event("key", 30, 1)  # A
                    input_event("key", 30, 0)
                    input_event("key", 46, 1)  # C
                    input_event("key", 46, 0)
                    input_event("key", 29, 0)
                    # Input roundtrips reach the compositor, not necessarily
                    # the app's clipboard publication. Wait for that evidence.
                    deadline = time.monotonic() + 3
                    while True:
                        clipboard = subprocess.run(["wl-paste", "--no-newline", "--seat", "mineral-test"], env=env, text=True,
                                                   capture_output=True, timeout=5)
                        if not clipboard.returncode or time.monotonic() >= deadline or "Nothing is copied" not in clipboard.stderr:
                            break
                        time.sleep(0.05)
                    if clipboard.returncode:
                        raise RuntimeError(f"Native clipboard read failed: {clipboard.stderr.strip()}")
                    copied = clipboard.stdout
                    expected = (args.copy_expect_file.read_text().removesuffix("\n")
                                if args.copy_expect_file is not None else None)
                    result = check_copied_text(copied, args.copy_order_check, expected)
                    result.update(
                        clipboard_sha256=hashlib.sha256(copied.encode()).hexdigest(),
                        expected_sha256=(hashlib.sha256(expected.encode()).hexdigest()
                                         if expected is not None else None),
                        binary_sha256=binary_hash,
                    )
                    args.output.with_suffix(".copy.json").write_text(json.dumps(result, indent=2) + "\n")
                    print(json.dumps(result), flush=True)
                    if not result["passes"]:
                        if expected is not None:
                            args.output.with_suffix(".copy-actual.txt").write_text(copied)
                        raise RuntimeError("Native copy lost, duplicated, reordered, or changed semantic text")
                    input_event("key", 106, 1)  # Right collapses the selection without editing.
                    input_event("key", 106, 0)
                if args.perf_seconds:
                    sweep = None
                    sweep_samples = []
                    if args.perf_sweep_seconds:
                        nodes = active_tree
                        editor_index, = (i for i, node in enumerate(nodes)
                                         if node["role"] == "entry" and node["name"] == "Markdown document editor")
                        descendants = {editor_index}
                        for i, node in enumerate(nodes):
                            if node["parent"] in descendants:
                                descendants.add(i)
                        viewport = nodes[editor_index]["bounds"]
                        content_end = max(nodes[i]["bounds"]["y"] + nodes[i]["bounds"]["height"]
                                          for i in descendants if "bounds" in nodes[i])
                        distance = content_end - viewport["y"] - viewport["height"]
                        ticks = round(args.perf_sweep_seconds / .012)
                        sweep = dict(distance_px=distance, ticks=ticks,
                                     premeasurement_passes=2 if args.perf_warm_sweep else 0,
                                     pointer_x=viewport["x"] + 4,
                                     step_milli=round((distance + viewport["height"]) / ticks * 1000))
                        if distance <= 0:
                            raise RuntimeError("Full-document sweep requires complete scrollable geometry")
                        time.sleep(max(0, app_started_at + int(env["MINERAL_PERF_WARMUP_MS"]) / 1000
                                       - (2 * args.perf_sweep_seconds + 2 if args.perf_warm_sweep else 0)
                                       - time.monotonic()))
                    with subprocess.Popen([str(modules / "input-client"), "stream"], env=env,
                                          stdin=subprocess.PIPE, text=True) as stream:
                        # The document's leading inset avoids nested code/table
                        # scroll handlers consuming part of a document sweep.
                        pointer_x = round(sweep["pointer_x"]) if sweep else args.width // 2
                        stream.stdin.write(f"move {pointer_x} {args.height // 2}\n")
                        stream.stdin.flush()
                        deadline = max(
                            time.monotonic() + args.perf_seconds + 20
                            + (2 * args.perf_sweep_seconds if args.perf_warm_sweep else 0),
                            app_started_at + int(env["MINERAL_PERF_WARMUP_MS"]) / 1000
                            + args.perf_seconds + 20,
                        )
                        tick = 0
                        while app.poll() is None and time.monotonic() < deadline:
                            amount = 1000 if tick < 240 or ((tick - 240) // 160) % 2 == 1 else -1000
                            operation = "scroll" if args.perf_input == "wheel" else "continuous-scroll"
                            if args.perf_input == "continuous":
                                amount *= 6
                            if sweep:
                                if args.perf_warm_sweep and tick == 2 * sweep["ticks"]:
                                    time.sleep(max(0, app_started_at + int(env["MINERAL_PERF_WARMUP_MS"]) / 1000
                                                   - time.monotonic()))
                                amount = sweep["step_milli"] * (1 if (tick // sweep["ticks"]) % 2 == 0 else -1)
                                if tick % sweep["ticks"] == 0:
                                    stream.stdin.write("key 66 1\nkey 66 0\n")
                                    sweep_samples.append(dict(tick=tick, elapsed_seconds=time.monotonic() - app_started_at))
                            stream.stdin.write(f"{operation} 0 {amount}\n")
                            stream.stdin.flush()
                            tick += 1
                            time.sleep(0.012)
                        stream.stdin.close()
                        stream.wait(timeout=5)
                    if app.poll() is None or app.returncode:
                        raise RuntimeError("Performance app did not finish successfully")
                    # Fidelity evidence is independent of timing success. Keep
                    # it even when the report is absent or the FPS gate fails.
                    verify_source_unchanged()
                    report = json.loads((work / "perf.json").read_text())
                    fps = report["presentation"]["interval"]["samples"] / report["actual_duration_seconds"]
                    report.update(binary_sha256=binary_hash, average_presented_fps=fps, driver_events=tick,
                                  native_input_kind=args.perf_input,
                                  accessibility_active=accessibility_active,
                                  accessible_nodes=accessible_nodes or None,
                                  fixture=args.fixture,
                                  fixture_sha256=hashlib.sha256((work / "layout-fixtures" / args.fixture).read_bytes()).hexdigest(),
                                  output_width_px=args.width, output_height_px=args.height,
                                  display_scale_120=args.scale, zoom_steps=args.zoom_steps)
                    report["source_resources"] = resource_evidence
                    if accessibility_sample is not None:
                        report["accessibility_activation_sample"] = {
                            key: value for key, value in accessibility_sample.items()
                            if key != "samples"
                        }
                    if sweep:
                        from resize_layout_check import prefixed_records
                        sweep["boundary_states"] = prefixed_records((work / "weston.log").read_bytes(), "MINERAL_RESIZE_STATE ")
                        sweep["boundary_inputs"] = sweep_samples
                        report["full_document_sweep"] = sweep
                    report["startup_wait_seconds"] = args.startup_wait
                    report["configured_warmup_ms"] = int(env["MINERAL_PERF_WARMUP_MS"])
                    report["accessibility_readiness_seconds"] = accessibility_readiness_seconds
                    report["passes"] = (fps > 60 and report["draw"]["p99_ms"] < 1000 / 60
                                        and report["input"]["latency"]["samples"] > 0)
                    args.output.with_suffix(".json").write_text(json.dumps(report, indent=2) + "\n")
                    print(f"Isolated Wayland: {fps:.1f} FPS; draw p99 {report['draw']['p99_ms']:.2f} ms; pass={report['passes']}", flush=True)
                    if not report["passes"]:
                        raise RuntimeError("Isolated scroll performance gate failed")
                    return
                if args.coast_check:
                    from PIL import Image

                    input_event("move", args.width // 2, args.height // 2)
                    operation = "scroll" if args.coast_check == "wheel" else "continuous-scroll"
                    count, amount = (4, 1000) if args.coast_check == "wheel" else (8, 6000)
                    for index in range(count):
                        input_event(operation, 0, amount)
                        if index + 1 < count:
                            time.sleep(0.016)
                    released = time.monotonic()
                    samples = []
                    for index, delay in enumerate([0.08, 0.28, 0.48, 0.78, 1.18, 2.8, 3.2]):
                        time.sleep(max(0, released + delay - time.monotonic()))
                        capture = work / f"coast-{index}"
                        capture.mkdir()
                        subprocess.run(["weston-screenshooter"], cwd=capture, env=env, check=True, timeout=10)
                        elapsed = time.monotonic() - released
                        screenshot, = capture.glob("*.png")
                        shutil.copyfile(screenshot, args.output.with_name(f"{args.output.stem}-{index}.png"))
                        with Image.open(screenshot) as source:
                            pixels = source.convert("RGB")
                            # Only the outer scrollbar rail, excluding caret,
                            # text and other independently changing content.
                            ys = [y for y in range(40, pixels.height - 5)
                                  if (lambda c: max(c) - min(c) < 5 and 90 <= c[0] <= 235)(
                                      pixels.getpixel((pixels.width - 7, y)))]
                        if len(ys) < 12:
                            raise RuntimeError("No measurable document scrollbar thumb")
                        samples.append({"seconds_after_release": elapsed, "thumb_top_px": min(ys)})
                    tops = [sample["thumb_top_px"] for sample in samples]
                    early_speed = (tops[2] - tops[0]) / (samples[2]["seconds_after_release"] - samples[0]["seconds_after_release"])
                    late_speed = (tops[4] - tops[2]) / (samples[4]["seconds_after_release"] - samples[2]["seconds_after_release"])
                    passes = (all(b + 1 >= a for a, b in zip(tops, tops[1:]))
                              and tops[3] > tops[2] + 1 and 0 < late_speed < early_speed
                              and abs(tops[-1] - tops[-2]) <= 1)
                    report = {"input": args.coast_check, "samples": samples, "passes": passes,
                              "early_thumb_speed_px_s": early_speed, "late_thumb_speed_px_s": late_speed,
                              "binary_sha256": binary_hash}
                    args.output.with_suffix(".json").write_text(json.dumps(report, indent=2) + "\n")
                    print(json.dumps(report), flush=True)
                    if not passes:
                        raise RuntimeError("Scroll did not visibly coast, decelerate and settle after input stopped")
                    return
                if args.layout_probes:
                    from PIL import Image

                    probe_points = list(zip(args.layout_probes[::2], args.layout_probes[1::2]))
                    probe_capture = work / "layout-before"
                    probe_capture.mkdir()
                    subprocess.run(["weston-screenshooter"], cwd=probe_capture, env=env, check=True, timeout=10)
                    probe_image, = probe_capture.glob("*.png")
                    shutil.copyfile(probe_image, args.output.with_name(f"{args.output.stem}-before.png"))
                    with Image.open(probe_image) as source:
                        pixels = source.convert("RGB")
                        if any(not (0 <= x < pixels.width and 0 <= y < pixels.height) for x, y in probe_points):
                            raise RuntimeError("Layout probe is outside the native screenshot")
                        before_probe_colors = [pixels.getpixel(point) for point in probe_points]
                if args.disclosure_check:
                    from PIL import Image, ImageChops

                    source_path = work / "layout-fixtures" / args.fixture
                    original = source_path.read_bytes()
                    x, y = args.disclosure_check
                    crop = (24, y + 45, args.width - 16, args.height - 16)
                    if crop[1] >= crop[3]:
                        raise RuntimeError("Disclosure check needs visible body space below the summary")
                    def disclosure_capture(suffix):
                        input_event("move", 10, 10)
                        time.sleep(1.2)
                        capture = work / f"disclosure-{suffix}"
                        capture.mkdir()
                        subprocess.run(["weston-screenshooter"], cwd=capture, env=env, check=True, timeout=10)
                        screenshot, = capture.glob("*.png")
                        shutil.copyfile(screenshot, args.output.with_name(f"{args.output.stem}-{suffix}.png"))
                        if source_path.read_bytes() != original:
                            raise RuntimeError("Disclosure interaction changed authored HTML/Markdown")
                        with Image.open(screenshot) as source:
                            return source.convert("RGB").crop(crop)
                    before = disclosure_capture("before")
                    input_event("move", x, y)
                    input_event("button", 272, 1)
                    input_event("button", 272, 0)
                    opened = disclosure_capture("opened")
                    difference = ImageChops.difference(before, opened)
                    changed = sum(pixel != (0, 0, 0) for pixel in difference.getdata())
                    if changed < 300:
                        raise RuntimeError("Mouse activation did not visibly expand disclosure body/layout")
                    input_event("key", 28, 1)  # Enter on the focused summary
                    input_event("key", 28, 0)
                    closed = disclosure_capture("closed")
                    if ImageChops.difference(before, closed).getbbox() is not None:
                        raise RuntimeError("Enter did not restore the exact closed body/following-content geometry")
                    input_event("key", 57, 1)  # Space on the focused summary
                    input_event("key", 57, 0)
                    reopened = disclosure_capture("space-opened")
                    if ImageChops.difference(opened, reopened).getbbox() is not None:
                        raise RuntimeError("Space did not reproduce the expanded body geometry")
                    input_event("key", 15, 1)  # Tab away from the summary
                    input_event("key", 15, 0)
                    time.sleep(0.15)
                    input_event("key", 42, 1)  # Shift+Tab back
                    input_event("key", 15, 1)
                    input_event("key", 15, 0)
                    input_event("key", 42, 0)
                    time.sleep(0.15)
                    input_event("key", 28, 1)
                    input_event("key", 28, 0)
                    tab_closed = disclosure_capture("tab-return-closed")
                    if ImageChops.difference(before, tab_closed).getbbox() is not None:
                        raise RuntimeError("Tab/Shift+Tab did not return focus to the summary for Enter activation")
                    report = dict(binary_sha256=binary_hash, native_mouse_open=True,
                                  native_enter_close=True, native_space_open=True,
                                  native_tab_return_activate=True,
                                  changed_body_pixels=changed, exact_closed_geometry=True,
                                  source_unchanged=True, width=args.width, height=args.height, zoom_steps=args.zoom_steps)
                    args.output.with_suffix(".disclosure.json").write_text(json.dumps(report, indent=2) + "\n")
                    print(json.dumps(report), flush=True)
                    return
                if args.html_anchor_check:
                    from PIL import Image
                    source_path = work / "layout-fixtures" / args.fixture
                    original = source_path.read_bytes()
                    def capture_anchor(stage):
                        capture = work / f"html-anchor-{stage}"
                        capture.mkdir()
                        subprocess.run(["weston-screenshooter"], cwd=capture, env=env, check=True, timeout=10)
                        screenshot, = capture.glob("*.png")
                        shutil.copyfile(screenshot, args.output.with_name(f"{args.output.stem}-{stage}.png"))
                        with Image.open(screenshot) as source:
                            pixels = source.convert("RGB")
                            rows = [y for y in range(pixels.height)
                                    if sum(pixels.getpixel((x, y)) == (224, 238, 232)
                                           for x in range(pixels.width)) >= 30]
                        return rows
                    if capture_anchor("before"):
                        raise RuntimeError("The target body was not initially closed")
                    input_event("move", *args.html_anchor_check)
                    if not args.html_anchor_keyboard:
                        input_event("key", 29, 1)
                    input_event("button", 272, 1)
                    input_event("button", 272, 0)
                    if args.html_anchor_keyboard:
                        input_event("key", 56, 1)
                        input_event("key", 28, 1)
                        input_event("key", 28, 0)
                        input_event("key", 56, 0)
                    else:
                        input_event("key", 29, 0)
                    time.sleep(1.2)
                    rows = capture_anchor("revealed")
                    if len(rows) < 20 or min(rows) > 180:
                        raise RuntimeError("HTML anchor did not reveal and scroll to its target")
                    if source_path.read_bytes() != original:
                        raise RuntimeError("HTML anchor navigation changed authored source")
                    result = dict(binary_sha256=binary_hash, fixture=args.fixture,
                                  width=args.width, height=args.height, zoom_steps=args.zoom_steps,
                                  activation="keyboard" if args.html_anchor_keyboard else "pointer",
                                  target_top_px=min(rows), target_visible_rows=len(rows),
                                  source_unchanged=True, nested_body_revealed=True)
                    args.output.with_suffix(".anchor.json").write_text(json.dumps(result, indent=2) + "\n")
                    print(json.dumps(result), flush=True)
                if args.follow_link:
                    if not args.follow_marker:
                        raise RuntimeError("--follow-link requires --follow-marker")
                    origin = work / "layout-fixtures" / args.fixture
                    target = work / "layout-fixtures" / (args.follow_fixture or args.fixture)
                    if not target.resolve().is_relative_to((work / "layout-fixtures").resolve()):
                        raise RuntimeError("Link verification is restricted to the private synthetic fixtures")
                    original_origin, original_target = origin.read_bytes(), target.read_bytes()
                    marker = args.follow_marker.encode()
                    if original_target.count(marker) != 1:
                        raise RuntimeError("The destination marker must be unique")
                    input_event("move", *args.follow_link)
                    input_event("key", 29, 1)
                    input_event("button", 272, 1)
                    input_event("button", 272, 0)
                    input_event("key", 29, 0)
                    time.sleep(1.2)
                    if origin.read_bytes() != original_origin or target.read_bytes() != original_target:
                        raise RuntimeError("Navigation modified source before typing")
                    capture = work / "link-destination"
                    capture.mkdir()
                    subprocess.run(["weston-screenshooter"], cwd=capture, env=env, check=True, timeout=10)
                    screenshot, = capture.glob("*.png")
                    shutil.copyfile(screenshot, args.output.with_name(f"{args.output.stem}-destination.png"))
                    input_event("key", 45, 1)  # X
                    input_event("key", 45, 0)
                    expected = original_target.replace(marker, b"x" + marker)
                    deadline = time.monotonic() + 6
                    while target.read_bytes() != expected and time.monotonic() < deadline:
                        time.sleep(0.05)
                    if target.read_bytes() != expected:
                        raise RuntimeError("The link did not place the caret at the exact destination heading")
                    if origin != target and origin.read_bytes() != original_origin:
                        raise RuntimeError("Destination typing modified the origin document")
                    input_event("key", 29, 1)
                    input_event("key", 44, 1)  # Z
                    input_event("key", 44, 0)
                    input_event("key", 29, 0)
                    deadline = time.monotonic() + 6
                    while target.read_bytes() != original_target and time.monotonic() < deadline:
                        time.sleep(0.05)
                    if target.read_bytes() != original_target or origin.read_bytes() != original_origin:
                        raise RuntimeError("One undo did not restore exact source bytes")
                    report = dict(binary_sha256=binary_hash, origin=args.fixture,
                                  destination=args.follow_fixture or args.fixture,
                                  native_navigation=True, exact_destination_typing=True,
                                  navigation_source_unchanged=True, exact_undo=True,
                                  width=args.width, height=args.height, zoom_steps=args.zoom_steps)
                    args.output.with_suffix(".navigation.json").write_text(json.dumps(report, indent=2) + "\n")
                    print(json.dumps(report), flush=True)
                    return
                if args.html_clipboard_check:
                    from html_clipboard_check import check
                    report = check(env, input_event, work / "layout-fixtures" / args.fixture,
                                   app.pid, ROOT / "performance/accessibility_probe.py")
                    report.update(binary_sha256=binary_hash, width=args.width, height=args.height, zoom_steps=args.zoom_steps)
                    args.output.with_suffix(".clipboard.json").write_text(json.dumps(report, indent=2) + "\n")
                    print(json.dumps(report), flush=True)
                if args.html_link_check:
                    source_path = work / "layout-fixtures" / args.fixture
                    original = source_path.read_bytes()
                    input_event("move", *args.html_link_check)
                    time.sleep(1.2)
                    for suffix in ["hover", "menu"]:
                        if suffix == "menu":
                            input_event("button", 273, 1)
                            input_event("button", 273, 0)
                            time.sleep(0.4)
                        capture = work / f"html-link-{suffix}"
                        capture.mkdir()
                        subprocess.run(["weston-screenshooter"], cwd=capture, env=env, check=True, timeout=10)
                        screenshot, = capture.glob("*.png")
                        shutil.copyfile(screenshot, args.output.with_name(f"{args.output.stem}-{suffix}.png"))
                    # Open link is first; Copy link address is second. Do not
                    # navigate to an external site during this native check.
                    for _ in range(2):
                        input_event("key", 108, 1)
                        input_event("key", 108, 0)
                    input_event("key", 28, 1)
                    input_event("key", 28, 0)
                    time.sleep(0.5)
                    clipboard = subprocess.run(["wl-paste", "--no-newline", "--seat", "mineral-test"],
                                               env=env, text=True, capture_output=True, timeout=5)
                    copied = clipboard.returncode == 0 and clipboard.stdout == "https://example.test/reference"
                    unchanged = source_path.read_bytes() == original
                    result = dict(native_copy_link=copied, source_unchanged=unchanged,
                                  width=args.width, height=args.height, zoom_steps=args.zoom_steps,
                                  external_navigation_tested=False, binary_sha256=binary_hash)
                    args.output.with_suffix(".link.json").write_text(json.dumps(result, indent=2) + "\n")
                    print(json.dumps(result), flush=True)
                    if not copied or not unchanged:
                        raise RuntimeError("Native HTML link command or source preservation failed")
                if args.convert_html_check:
                    source_path = work / "layout-fixtures" / args.fixture
                    original = source_path.read_bytes()
                    def wait_source(predicate):
                        deadline = time.monotonic() + 5
                        while time.monotonic() < deadline:
                            value = source_path.read_bytes()
                            if predicate(value):
                                return value
                            time.sleep(0.05)
                        raise RuntimeError("HTML conversion/typing/undo did not reach autosave")
                    def undo():
                        input_event("key", 29, 1)
                        input_event("key", 44, 1)
                        input_event("key", 44, 0)
                        input_event("key", 29, 0)
                    input_event("move", *args.convert_html_check)
                    button = 273 if args.html_edit_here else 272
                    input_event("button", button, 1)
                    input_event("button", button, 0)
                    if args.html_edit_here:
                        time.sleep(0.4)
                        if source_path.read_bytes() != original:
                            raise RuntimeError("Opening HTML commands changed the source")
                        capture = work / "html-menu"
                        capture.mkdir()
                        subprocess.run(["weston-screenshooter"], cwd=capture, env=env, check=True, timeout=10)
                        screenshot, = capture.glob("*.png")
                        shutil.copyfile(screenshot, args.output.with_name(f"{args.output.stem}-menu.png"))
                        # The dedicated native menu has one command. Invoke
                        # through keyboard to exercise its action/focus route.
                        input_event("key", 108, 1)  # Down
                        input_event("key", 108, 0)
                        input_event("key", 28, 1)   # Enter
                        input_event("key", 28, 0)
                    if args.html_direct_edit:
                        time.sleep(0.5)
                        if source_path.read_bytes() != original:
                            raise RuntimeError("Selecting rendered HTML changed its source")
                        capture = work / "html-caret"
                        capture.mkdir()
                        subprocess.run(["weston-screenshooter"], cwd=capture, env=env, check=True, timeout=10)
                        screenshot, = capture.glob("*.png")
                        shutil.copyfile(screenshot, args.output.with_name(f"{args.output.stem}-caret.png"))
                        converted = original
                    else:
                        converted = wait_source(lambda value: value != original)
                    if not args.html_direct_edit and (b"**A styled HTML fragment**" not in converted or b"[reference link](https://example.test/reference)" not in converted):
                        raise RuntimeError("HTML conversion lost rich text or its link")
                    if args.fixture == "30-html-images.md" and not args.html_direct_edit:
                        required = [b"![Layered mineral strata](../visual-assets/mineral-strata.svg", b"](02-list-arrangements.md)", b"![Repeated source\\, second placement](../visual-assets/mineral-strata.svg)"]
                        if not all(marker in converted for marker in required):
                            args.output.with_suffix(".conversion-failed.md").write_bytes(converted)
                            raise RuntimeError("HTML image conversion lost an image, alt text or enclosing link")
                    insertion = (args.edit_paste.encode() if args.edit_paste is not None
                                 else "é".encode() if args.html_compose_check else b"x")
                    if args.html_compose_check:
                        input_event("key", 40, 1)  # dead acute on the private US International keymap
                        input_event("key", 40, 0)
                        time.sleep(1.2)
                        if source_path.read_bytes() != original:
                            raise RuntimeError("Provisional HTML composition reached autosave")
                        capture = work / "html-preedit"
                        capture.mkdir()
                        subprocess.run(["weston-screenshooter"], cwd=capture, env=env, check=True, timeout=10)
                        screenshot, = capture.glob("*.png")
                        shutil.copyfile(screenshot, args.output.with_name(f"{args.output.stem}-preedit.png"))
                    if args.edit_paste is not None:
                        subprocess.run(["wl-copy", "--seat", "mineral-test", "--type", "text/plain"],
                                       input=args.edit_paste, text=True, env=env, check=True, timeout=5)
                        input_event("key", 29, 1)
                        input_event("key", 47, 1)
                        input_event("key", 47, 0)
                        input_event("key", 29, 0)
                    else:
                        key = 18 if args.html_compose_check else 45  # E completes é; otherwise X
                        input_event("key", key, 1)
                        input_event("key", key, 0)
                    typed = wait_source(lambda value: value != converted)
                    if args.fixture == "30-html-images.md" and args.html_direct_edit:
                        required = [b"![Layered mineral strata](../visual-assets/mineral-strata.svg", b"](02-list-arrangements.md)", b"![Repeated source\\, second placement](../visual-assets/mineral-strata.svg)"]
                        if not all(marker in typed for marker in required):
                            args.output.with_suffix(".conversion-failed.md").write_bytes(typed)
                            raise RuntimeError("First text edit lost an HTML image or its link")
                    if args.fixture == "23-disclosure-editing.md" and (b"<details" in typed or b"<summary" in typed):
                        raise RuntimeError("Disclosure edit did not enter the canonical Markdown conversion path")
                    link_label = b"reference link"
                    link_labels = ([link_label[:index] + insertion + link_label[index:]
                                    for index in range(1, len(link_label))]
                                   if args.html_direct_edit and args.html_edit_within == "reference link"
                                   else [link_label])
                    link_retained = any(b"[" + label + b"](https://example.test/reference)" in typed
                                        for label in link_labels)
                    if args.html_direct_edit and (b"**A styled HTML fragment**" not in typed or not link_retained or b'<div style="padding:16px' in typed):
                        raise RuntimeError("First direct edit did not convert HTML with rich text and its link intact")
                    if args.html_edit_within:
                        target = args.html_edit_within.encode()
                        if converted.count(target) != 1:
                            raise RuntimeError("HTML insertion target must be unique")
                        start = converted.index(target)
                        exact_insertion = any(typed == converted[:index] + insertion + converted[index:]
                                              for index in range(start + 1, start + len(target)))
                        converted_insertion = (target not in typed and any(
                            typed.count(target[:index] + insertion + target[index:]) == 1
                            for index in range(1, len(target))))
                        if not (converted_insertion if args.html_direct_edit else exact_insertion):
                            raise RuntimeError("HTML edit did not land inside the clicked text")
                    elif insertion + b"Six independent ideas" not in typed:
                        raise RuntimeError("Conversion did not focus the new editable text")
                    capture = work / "html-typed"
                    capture.mkdir()
                    subprocess.run(["weston-screenshooter"], cwd=capture, env=env, check=True, timeout=10)
                    screenshot, = capture.glob("*.png")
                    shutil.copyfile(screenshot, args.output.with_name(f"{args.output.stem}-typed.png"))
                    if args.edit_blur:
                        input_event("move", *args.edit_blur)
                        input_event("button", 272, 1)
                        input_event("button", 272, 0)
                        time.sleep(max(1.5, args.reflow_wait))
                        if source_path.read_bytes() != typed:
                            raise RuntimeError("HTML conversion blur/reflow changed Markdown")
                        capture = work / "html-blurred"
                        capture.mkdir()
                        subprocess.run(["weston-screenshooter"], cwd=capture, env=env, check=True, timeout=10)
                        screenshot, = capture.glob("*.png")
                        shutil.copyfile(screenshot, args.output.with_name(f"{args.output.stem}-blurred.png"))
                    undo()
                    wait_source(lambda value: value == converted)
                    if not args.html_direct_edit:
                        undo()
                        wait_source(lambda value: value == original)
                    result = dict(fixture=args.fixture, native_conversion=True,
                                  edit_here=args.html_edit_here, edit_within=args.html_edit_within,
                                  opening_menu_preserves_source=args.html_edit_here,
                                  click_preserves_source=args.html_direct_edit,
                                  preedit_preserves_source=True if args.html_compose_check else None,
                                  ime_transport="native XKB US International dead-key compose" if args.html_compose_check else None,
                                  width=args.width, height=args.height, zoom_steps=args.zoom_steps,
                                  formatting_and_link_retained=True, typed_into_converted_text=True,
                                  edit_paste_bytes=len(args.edit_paste.encode()) if args.edit_paste else 0,
                                  blur_preserves_source=True if args.edit_blur else None,
                                  undos_to_restore_exact_html=1 if args.html_direct_edit else 2,
                                  binary_sha256=binary_hash)
                    args.output.with_suffix(".conversion.json").write_text(json.dumps(result, indent=2) + "\n")
                    print(json.dumps(result), flush=True)
                    time.sleep(0.5)
                if args.table_resize_check:
                    from table_resize_check import check_resize
                    result = check_resize(env, app.pid, work, args.output, view_source_path,
                                          ROOT / "performance/accessibility_probe.py",
                                          input_event, args.select, 1 + args.zoom_steps * 0.1)
                    result.update(binary_sha256=binary_hash, fixture=args.fixture,
                                  width=args.width, height=args.height, zoom_steps=args.zoom_steps)
                    args.output.with_suffix(".table-resize.json").write_text(json.dumps(result, indent=2) + "\n")
                    print(json.dumps(result), flush=True)
                if args.task_toggle_check:
                    from task_toggle_check import check_toggle
                    report = check_toggle(env, work, args.output, view_source_path, input_event,
                                          args.select, args.task_toggle_check)
                    report.update(binary_sha256=binary_hash, fixture=args.fixture,
                                  width=args.width, height=args.height, zoom_steps=args.zoom_steps)
                    args.output.with_suffix(".task-toggle.json").write_text(json.dumps(report, indent=2) + "\n")
                    print(json.dumps(report))
                if args.select and not (args.table_resize_check or args.task_toggle_check):
                    selection_source = (work / "layout-fixtures" / args.fixture).read_bytes()
                    x1, y1, x2, y2 = args.select
                    input_event("move", x1, y1)
                    input_event("button", 272, 1)
                    input_event("move", x2, y2)
                    time.sleep(args.select_hold_seconds)
                    input_event("button", 272, 0)
                    time.sleep(0.5)
                    for key in args.selection_keys or []:
                        for code, value in selection_key_events(key):
                            input_event("key", code, value)
                        time.sleep(0.08)
                    if args.copy_selected is not None:
                        capture = work / "selected-range"
                        capture.mkdir()
                        subprocess.run(["weston-screenshooter"], cwd=capture, env=env, check=True, timeout=10)
                        screenshot, = capture.glob("*.png")
                        shutil.copyfile(screenshot, args.output.with_name(f"{args.output.stem}-selected.png"))
                        input_event("key", 29, 1)
                        input_event("key", 46, 1)
                        input_event("key", 46, 0)
                        input_event("key", 29, 0)
                        deadline = time.monotonic() + 3
                        while True:
                            clipboard = subprocess.run(["wl-paste", "--no-newline", "--seat", "mineral-test"],
                                                       env=env, text=True, capture_output=True, timeout=5)
                            if clipboard.stdout == args.copy_selected or time.monotonic() >= deadline:
                                break
                            time.sleep(0.05)
                        unchanged = (work / "layout-fixtures" / args.fixture).read_bytes() == selection_source
                        passed = not clipboard.returncode and clipboard.stdout == args.copy_selected and unchanged
                        result = dict(selected_text=clipboard.stdout, exact_selection_copy=passed,
                                      selection_keys=args.selection_keys or [],
                                      stationary_drag_seconds=args.select_hold_seconds,
                                      source_unchanged=unchanged, binary_sha256=binary_hash)
                        args.output.with_suffix(".selection.json").write_text(json.dumps(result, indent=2) + "\n")
                        print(json.dumps(result), flush=True)
                        if not passed:
                            raise RuntimeError(f"Native selected-text copy or source preservation failed: "
                                               f"clipboard_status={clipboard.returncode}, clipboard_error={clipboard.stderr!r}, "
                                               f"app_status={app.poll()}")
                if args.edit_check:
                    source_path = work / "layout-fixtures" / args.fixture
                    original = source_path.read_bytes()
                    if args.edit_compose_check:
                        def accessibility_names():
                            result = json.loads(subprocess.run([
                                "/usr/bin/python3", str(ROOT / "performance/accessibility_probe.py"), str(app.pid)
                            ], env=env, capture_output=True, text=True, check=True, timeout=20).stdout)
                            return [node["name"] for node in result["nodes"]]

                        def verify_preedit(stage):
                            names_before = accessibility_names()
                            input_event("key", 40, 1)
                            input_event("key", 40, 0)
                            time.sleep(1.2)
                            capture = work / f"selection-{stage}"
                            capture.mkdir()
                            subprocess.run(["weston-screenshooter"], cwd=capture, env=env, check=True, timeout=10)
                            screenshot, = capture.glob("*.png")
                            shutil.copyfile(screenshot, args.output.with_name(f"{args.output.stem}-{stage}.png"))
                            names = accessibility_names()
                            standalone_count = names.count(args.edit_compose_check)
                            inline_matches = []
                            if args.edit_within:
                                for name in names:
                                    insertion = single_inserted_codepoint(args.edit_within, name)
                                    if insertion is not None:
                                        inline_matches.append(dict(
                                            name=name,
                                            offset=insertion[0],
                                            codepoint=insertion[1],
                                        ))
                            inline_target = (args.edit_within is not None
                                             and names_before.count(args.edit_within) == 1
                                             and len(inline_matches) == 1)
                            mode = ("inline_target" if inline_target
                                    else "standalone" if standalone_count == 1
                                    else None)
                            passed = mode is not None and source_path.read_bytes() == original
                            evidence = dict(expected_preedit=args.edit_compose_check, names=names,
                                            accessibility_mode=mode,
                                            target_before_count=(names_before.count(args.edit_within)
                                                                 if args.edit_within else None),
                                            standalone_count=standalone_count,
                                            inline_matches=inline_matches,
                                            source_unchanged=source_path.read_bytes() == original,
                                            passes=passed, binary_sha256=binary_hash)
                            args.output.with_suffix(f".{stage}.json").write_text(json.dumps(evidence, indent=2) + "\n")
                            if not passed:
                                raise RuntimeError(f"Native provisional text/source check failed: {evidence}")
                        verify_preedit("preedit")
                        if args.edit_compose_cancel_check:
                            input_event("key", 1, 1)  # Escape
                            input_event("key", 1, 0)
                            time.sleep(1.2)
                            subprocess.run(["wl-copy", "--seat", "mineral-test", "--type", "text/plain"],
                                           input="composition-cancel-sentinel", text=True, env=env, check=True, timeout=5)
                            for code, value in [(29, 1), (46, 1), (46, 0), (29, 0)]:
                                input_event("key", code, value)
                            deadline = time.monotonic() + 3
                            while True:
                                clipboard = subprocess.run(["wl-paste", "--no-newline", "--seat", "mineral-test"],
                                                           env=env, text=True, capture_output=True, timeout=5)
                                if clipboard.stdout == args.copy_selected or time.monotonic() >= deadline:
                                    break
                                time.sleep(0.05)
                            evidence = dict(selected_text=clipboard.stdout,
                                            exact_selection_copy=not clipboard.returncode and clipboard.stdout == args.copy_selected,
                                            source_unchanged=source_path.read_bytes() == original,
                                            binary_sha256=binary_hash)
                            args.output.with_suffix(".cancel.json").write_text(json.dumps(evidence, indent=2) + "\n")
                            capture = work / "selection-cancelled"
                            capture.mkdir()
                            subprocess.run(["weston-screenshooter"], cwd=capture, env=env, check=True, timeout=10)
                            screenshot, = capture.glob("*.png")
                            shutil.copyfile(screenshot, args.output.with_name(f"{args.output.stem}-cancelled.png"))
                            if not (evidence["exact_selection_copy"] and evidence["source_unchanged"]):
                                raise RuntimeError(f"Native cancellation lost source or selection: {evidence}")
                            verify_preedit("restart-preedit")
                    if args.edit_paste is None:
                        key = 18 if args.edit_compose_check else 45  # E completes é; otherwise X
                        input_event("key", key, 1)
                        input_event("key", key, 0)
                    else:
                        subprocess.run(["wl-copy", "--seat", "mineral-test", "--type", "text/plain"],
                                       input=args.edit_paste, text=True, env=env, check=True, timeout=5)
                        input_event("key", 29, 1)
                        if args.edit_markdown:
                            input_event("key", 42, 1)
                        input_event("key", 47, 1)  # Ctrl+V
                        input_event("key", 47, 0)
                        if args.edit_markdown:
                            input_event("key", 42, 0)
                        input_event("key", 29, 0)
                    deadline = time.monotonic() + 5
                    while source_path.read_bytes() == original and time.monotonic() < deadline:
                        time.sleep(0.05)
                    edited = source_path.read_bytes()
                    if edited == original:
                        raise RuntimeError("Native typing did not reach the document autosave path")
                    if args.edit_expect is not None and edited != args.edit_expect.encode():
                        raise RuntimeError(f"Native edited source mismatch: expected={args.edit_expect!r}, actual={edited.decode()!r}")
                    for preserved in args.edit_preserve or []:
                        fragment = preserved.encode()
                        if original.count(fragment) != 1 or edited.count(fragment) != 1:
                            location = edited.find(fragment[:16])
                            excerpt = edited[max(0, location - 32):max(0, location - 32) + 240]
                            raise RuntimeError(
                                f"Native editing changed or duplicated protected fragment: {preserved!r}; "
                                f"original_count={original.count(fragment)}, edited_count={edited.count(fragment)}, "
                                f"nearby_edited_source={excerpt!r}")
                    if args.edit_within:
                        marker = args.edit_within.encode()
                        # An edited Markdown node may canonically escape nearby
                        # punctuation (e.g. the comma after $...$). Assert the
                        # exact change inside the unique intended source span;
                        # the subsequent undo oracle still compares ALL bytes.
                        semantic_edited = unescape_markdown_punctuation(edited)
                        insertion = (args.edit_paste or ("é" if args.edit_compose_check else "x")).encode()
                        if not inserted_marker_matches(original, edited, marker, insertion):
                            changed = next((i for i, (a, b) in enumerate(zip(original, edited)) if a != b), min(len(original), len(edited)))
                            raise RuntimeError(f"Native typing missed exact source insertion: marker_count={original.count(marker)}, first_change={changed}, before={original[max(0, changed-24):changed+96]!r}, after={edited[max(0, changed-24):changed+96]!r}, semantic_after={semantic_edited[max(0, changed-24):changed+96]!r}")
                    # Keep the actual edited state, not only the restored
                    # document. Before/after-undo images cannot demonstrate
                    # topology stability while the user is typing.
                    capture = work / "typed-edit"
                    capture.mkdir()
                    subprocess.run(["weston-screenshooter"], cwd=capture, env=env, check=True, timeout=10)
                    screenshot, = capture.glob("*.png")
                    shutil.copyfile(screenshot, args.output.with_name(f"{args.output.stem}-typed.png"))
                    if args.edit_idle_seconds:
                        time.sleep(args.edit_idle_seconds)
                        capture = work / "idle-edit"
                        capture.mkdir()
                        subprocess.run(["weston-screenshooter"], cwd=capture, env=env, check=True, timeout=10)
                        screenshot, = capture.glob("*.png")
                        shutil.copyfile(screenshot, args.output.with_name(f"{args.output.stem}-idle.png"))
                        if source_path.read_bytes() != edited:
                            raise RuntimeError("Idle layout work changed the edited source")
                    if args.edit_zoom_steps:
                        input_event("key", 29, 1)
                        for _ in range(abs(args.edit_zoom_steps)):
                            key = 13 if args.edit_zoom_steps > 0 else 12
                            input_event("key", key, 1)
                            input_event("key", key, 0)
                            time.sleep(0.05)
                        input_event("key", 29, 0)
                        time.sleep(args.reflow_wait)
                        capture = work / "active-edit"
                        capture.mkdir()
                        subprocess.run(["weston-screenshooter"], cwd=capture, env=env, check=True, timeout=10)
                        screenshot, = capture.glob("*.png")
                        shutil.copyfile(screenshot, args.output.with_name(f"{args.output.stem}-editing.png"))
                    if args.edit_blur:
                        input_event("move", *args.edit_blur)
                        input_event("button", 272, 1)
                        input_event("button", 272, 0)
                        time.sleep(args.reflow_wait)
                        capture = work / "edit-blurred"
                        capture.mkdir()
                        subprocess.run(["weston-screenshooter"], cwd=capture, env=env, check=True, timeout=10)
                        screenshot, = capture.glob("*.png")
                        shutil.copyfile(screenshot, args.output.with_name(f"{args.output.stem}-blurred.png"))
                    input_event("key", 29, 1)
                    input_event("key", 44, 1)  # Ctrl+Z
                    input_event("key", 44, 0)
                    input_event("key", 29, 0)
                    deadline = time.monotonic() + 5
                    while source_path.read_bytes() != original and time.monotonic() < deadline:
                        time.sleep(0.05)
                    restored = source_path.read_bytes() == original
                    result = dict(fixture=args.fixture, zoom_steps=args.zoom_steps,
                                  native_type_autosaved=True, undo_restores_exact_bytes=restored,
                                  preedit_text=args.edit_compose_check,
                                  preedit_preserves_source=True if args.edit_compose_check else None,
                                  binary_sha256=binary_hash,
                                  edit_within=args.edit_within,
                                  expected_edited_source_exact=True if args.edit_expect is not None else None,
                                  preserved_fragments=args.edit_preserve or [],
                                  edit_markdown=args.edit_markdown,
                                  edit_paste_bytes=len(args.edit_paste.encode()) if args.edit_paste else 0,
                                  edit_zoom_steps=args.edit_zoom_steps,
                                  edit_idle_seconds=args.edit_idle_seconds,
                                  edit_blur=args.edit_blur,
                                  first_changed_byte=next((i for i, (a, b) in enumerate(zip(original, edited)) if a != b), min(len(original), len(edited))))
                    args.output.with_suffix(".edit.json").write_text(json.dumps(result, indent=2) + "\n")
                    if not restored:
                        raise RuntimeError("Native undo did not restore the original source bytes")
                    print(json.dumps(result), flush=True)
                if args.hover:
                    input_event("move", *args.hover)
                    time.sleep(1.2)
                if args.appearance_cycle:
                    from theme_check import check_cycle
                    result = check_cycle(env, app.pid, work, args.output,
                                         ROOT / "performance/accessibility_probe.py",
                                         view_source_path, args.appearance, input_event,
                                         selected=bool(args.select))
                    result.update(binary_sha256=binary_hash, fixture=args.fixture,
                                  width=args.width, height=args.height, zoom_steps=args.zoom_steps)
                    args.output.with_suffix(".appearance-cycle.json").write_text(json.dumps(result, indent=2) + "\n")
                    if not result["passes"]:
                        raise RuntimeError("Native system appearance cycle failed")
                subprocess.run(["weston-screenshooter"], cwd=work, env=env, check=True, timeout=10)
                screenshots = list(work.glob("*.png"))
                if len(screenshots) != 1:
                    raise RuntimeError(f"Expected one screenshot, got {screenshots}")
                shutil.copyfile(screenshots[0], args.output)
                if args.appearance:
                    from theme_check import check_image
                    result = check_image(args.output, args.appearance)
                    result.update(binary_sha256=binary_hash, fixture=args.fixture,
                                  width=args.width, height=args.height, zoom_steps=args.zoom_steps)
                    args.output.with_suffix(".appearance.json").write_text(json.dumps(result, indent=2) + "\n")
                    if not result["passes"]:
                        raise RuntimeError("Document/chrome did not follow OS appearance")
                if args.html_images_check:
                    from PIL import Image
                    with Image.open(screenshots[0]) as source:
                        colors = source.convert("RGB").getcolors(source.width * source.height)
                    counts = {color: count for count, color in colors}
                    bands = [(16, 25, 35), (211, 151, 60), (47, 143, 107), (169, 79, 69)]
                    unchanged = (work / "layout-fixtures" / args.fixture).read_bytes() == (ROOT / "performance/layout-fixtures" / args.fixture).read_bytes()
                    passed = unchanged and all(counts.get(color, 0) > 256 for color in bands)
                    report = dict(binary_sha256=binary_hash, width=args.width, height=args.height,
                                  zoom_steps=args.zoom_steps, source_unchanged=unchanged,
                                  image_band_pixels=[counts.get(color, 0) for color in bands], passes=passed)
                    args.output.with_suffix(".images.json").write_text(json.dumps(report, indent=2) + "\n")
                    print(json.dumps(report), flush=True)
                    if not passed:
                        raise RuntimeError("HTML images did not paint decoded source colors or changed authored source")
                if args.recovery_controls_check:
                    from status_controls_check import check
                    result = check(args.recovery_controls_check, env, input_event, app.pid,
                                   work / "layout-fixtures" / args.fixture,
                                   ROOT / "performance/accessibility_probe.py", args.output, work)
                    result["binary_sha256"] = binary_hash
                    args.output.with_suffix(".recovery-controls.json").write_text(json.dumps(result, indent=2) + "\n")
                    print(json.dumps(result), flush=True)
                if args.unsaved_close_check:
                    from titlebar_check import unsaved_close
                    result = unsaved_close(args.unsaved_close_check, env, input_event, app,
                                           work / "layout-fixtures" / args.fixture,
                                           ROOT / "performance/accessibility_probe.py", args.output, work)
                    result["binary_sha256"] = binary_hash
                    args.output.with_suffix(".unsaved-close.json").write_text(json.dumps(result, indent=2) + "\n")
                    print(json.dumps(result), flush=True)
                if args.window_controls_check:
                    from titlebar_check import window_controls
                    result = window_controls(env, input_event, app,
                                             work / "layout-fixtures" / args.fixture,
                                             ROOT / "performance/accessibility_probe.py", args.output, work)
                    result["binary_sha256"] = binary_hash
                    args.output.with_suffix(".window-controls.json").write_text(json.dumps(result, indent=2) + "\n")
                    print(json.dumps(result), flush=True)
                if args.titlebar_check:
                    source_path = work / "layout-fixtures" / args.fixture
                    original = source_path.read_bytes()
                    input_event("move", args.width - 14, 16)
                    input_event("button", 272, 1)
                    input_event("button", 272, 0)
                    deadline = time.monotonic() + 4
                    while app.poll() is None and time.monotonic() < deadline:
                        time.sleep(0.05)
                    passed = app.poll() == 0 and source_path.read_bytes() == original
                    report = dict(binary_sha256=binary_hash, fixture=args.fixture,
                                  width=args.width, height=args.height, zoom_steps=args.zoom_steps,
                                  native_close_inside_window=app.poll() == 0,
                                  display_scale_120=args.scale, states=titlebar_states,
                                  source_unchanged=source_path.read_bytes() == original, passes=passed)
                    args.output.with_suffix(".titlebar.json").write_text(json.dumps(report, indent=2) + "\n")
                    print(json.dumps(report), flush=True)
                    if not passed:
                        raise RuntimeError("The titlebar close button is not reachable inside the window")
                if args.layout_probes:
                    with Image.open(screenshots[0]) as source:
                        pixels = source.convert("RGB")
                        after_probe_colors = [pixels.getpixel(point) for point in probe_points]
                    stable = all(max(abs(a - b) for a, b in zip(before, after)) <= 3
                                 for before, after in zip(before_probe_colors, after_probe_colors))
                    if args.probe_color:
                        stable &= all(max(abs(a - b) for a, b in zip(color, args.probe_color)) <= 3
                                      for color in before_probe_colors + after_probe_colors)
                    result = dict(points=probe_points, before=before_probe_colors, after=after_probe_colors,
                                  expected=args.probe_color, stable=stable,
                                  binary_sha256=binary_hash)
                    args.output.with_suffix(".layout.json").write_text(json.dumps(result, indent=2) + "\n")
                    print(json.dumps(result), flush=True)
                    if not stable:
                        raise RuntimeError("Native layout surfaces moved/disappeared during the interaction")
                verify_source_unchanged()
                print(args.output)
            finally:
                for process in [app, weston]:
                    if process is not None and process.poll() is None:
                        process.terminate()
                        process.wait(timeout=10)
                if args.log_output:
                    log.flush()
                    args.log_output.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(work / "weston.log", args.log_output)
                if args.layout_trace:
                    log.flush()
                    prefix = "MINERAL_LAYOUT_TRACE "
                    reports = [json.loads(line[len(prefix):]) for line in (work / "weston.log").read_text().splitlines()
                               if line.startswith(prefix)]
                    result = dict(binary_sha256=binary_hash, fixture=args.fixture, width=args.width,
                                  height=args.height, display_scale_120=args.scale, zoom_steps=args.zoom_steps,
                                  trace_mode=args.layout_trace, reports=reports)
                    args.output.with_suffix(".planning.json").write_text(json.dumps(result, indent=2) + "\n")
                    if not any(report["committed"] and report["original_source_bytes"] == fixture_bytes for report in reports) and sys.exc_info()[0] is None:
                        raise RuntimeError("No committed measured layout was captured")
                    for report in reports:
                        if report["committed"] == bool(report["discard_reason"]):
                            raise RuntimeError("Planning outcome and discard reason disagree")
                        if report["scope"] != "whole_document":
                            raise RuntimeError("Update the harness for the new planning scope")
                    if args.cached_geometry_check:
                        check_cached_geometry(reports, args.layout_samples)
                    print(json.dumps(dict(planning_reports=len(reports),
                                          committed=sum(report["committed"] for report in reports),
                                          worker_ms=[round(report["worker_ms"], 3) for report in reports])), flush=True)


if __name__ == "__main__":
    main()
