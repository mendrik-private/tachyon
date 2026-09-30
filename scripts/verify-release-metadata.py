#!/usr/bin/env python3
"""Verify that a release tag matches Tachyon's packaged metadata."""

from __future__ import annotations

import argparse
import re
import tomllib
import xml.etree.ElementTree as ET
from pathlib import Path


SEMVER = re.compile(
    r"(?:0|[1-9][0-9]*)\."
    r"(?:0|[1-9][0-9]*)\."
    r"(?:0|[1-9][0-9]*)"
    r"(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
)


def fail(message: str) -> None:
    raise SystemExit(f"error: {message}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="verify a vVERSION tag against Cargo and AppStream metadata"
    )
    parser.add_argument("tag", help="release tag, for example v0.1.15")
    parser.add_argument(
        "--project-root",
        type=Path,
        default=Path(__file__).resolve().parent.parent,
        help=argparse.SUPPRESS,
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    root = args.project_root.resolve()
    manifest_path = root / "crates/markdown-app/Cargo.toml"
    lock_path = root / "Cargo.lock"
    appstream_path = root / "packaging/io.github.mendrik_private.Tachyon.metainfo.xml"

    manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    version = manifest["package"]["version"]
    if not isinstance(version, str) or SEMVER.fullmatch(version) is None:
        fail(f"markdown-app has unsupported release version {version!r}")

    expected_tag = f"v{version}"
    if args.tag != expected_tag:
        fail(f"release tag {args.tag!r} does not match {expected_tag!r}")

    lock = tomllib.loads(lock_path.read_text(encoding="utf-8"))
    locked_versions = [
        package.get("version")
        for package in lock.get("package", [])
        if package.get("name") == "markdown-app"
    ]
    if locked_versions != [version]:
        fail(
            "Cargo.lock must contain exactly one markdown-app package at "
            f"version {version}; found {locked_versions!r}"
        )

    releases = ET.parse(appstream_path).getroot().find("releases")
    if releases is None or len(releases) == 0:
        fail("AppStream metadata has no releases")
    appstream_versions = [release.get("version") for release in releases]
    if appstream_versions[0] != version:
        fail(
            "the newest AppStream release must match markdown-app version "
            f"{version}; found {appstream_versions[0]!r}"
        )
    if appstream_versions.count(version) != 1:
        fail(f"AppStream release version {version} must occur exactly once")

    print(version)


if __name__ == "__main__":
    try:
        main()
    except (KeyError, OSError, ET.ParseError, tomllib.TOMLDecodeError) as error:
        fail(str(error))
