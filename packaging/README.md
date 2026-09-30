# Linux packaging

The desktop identity is `io.github.mendrik_private.Tachyon`. A distributor should install:

- the `tachyon` release binary under `bin/`;
- `io.github.mendrik_private.Tachyon.desktop` under `share/applications/`;
- `io.github.mendrik_private.Tachyon.metainfo.xml` under `share/metainfo/`; and
- the icon tree under `share/icons/hicolor/`; and
- the MIT and Apache-2.0 license texts under `share/licenses/tachyon/`.

The binary is Wayland-only by product contract. The bundled fonts and their
licenses are compiled into the executable from `assets/fonts/`.

HTML previews keep the bundled Spline fonts as their default and use installed
fonts through Fontconfig for additional scripts and emoji. Distributions need
fonts covering the languages they support; no font files are downloaded from
documents. The native multilingual validation uses installed Noto CJK, Arabic
and Color Emoji fonts. After installing or changing fonts, restart Tachyon to
rebuild its process-local font collection and retained previews.

The application icon is the supplied lightning/Markdown PNG, preserved at its
original resolution. GNOME scales it to the requested launcher size. Its icon
name and desktop filename match the Wayland application ID.

Build and stage a complete installation under an explicit prefix:

```sh
cargo build --release --locked --bin tachyon
packaging/install.sh /tmp/tachyon-stage/usr
```

The installer rejects an empty prefix and `/`; it never selects a system or
user prefix implicitly. Validate the staged desktop integration with:

```sh
desktop-file-validate /tmp/tachyon-stage/usr/share/applications/io.github.mendrik_private.Tachyon.desktop
appstreamcli validate --no-net /tmp/tachyon-stage/usr/share/metainfo/io.github.mendrik_private.Tachyon.metainfo.xml
file /tmp/tachyon-stage/usr/share/icons/hicolor/scalable/apps/io.github.mendrik_private.Tachyon.png
```

## GitHub release workflow

[Release](../.github/workflows/release.yml) builds a Linux x86_64 archive on
Ubuntu 24.04. A pushed `v*` tag starts the workflow. The tag must exactly match
`v` plus the version in `crates/markdown-app/Cargo.toml`, including any prerelease
suffix. Rerun a failed tagged workflow from GitHub Actions; the release workflow
does not accept a branch or manual dispatch.

Before tagging, commit the complete application and its build inputs: manifests,
lockfiles, vendored sources, bundled fonts, test fixtures, documentation and
packaging assets. GitHub builds only the tagged commit; local untracked files
are not available to the runner. The Rust toolchain pinned in
`rust-toolchain.toml` must be downloadable on the runner.

The workflow runs `scripts/check.sh`, builds the optimized binary with `--locked`,
stages it with `packaging/install.sh`, validates the desktop entry and AppStream
metadata, and checks for unresolved shared libraries. The resulting release
contains:

- `tachyon-vVERSION-linux-x86_64.tar.gz`, with `bin/`, desktop integration,
  licenses, source documentation, and the offline HTML user guide under
  `share/doc/tachyon/user-guide/`; the README's screenshot sources and capture
  provenance are also included at their documented relative paths;
- `runtime-libraries.txt`, the build host's shared-library dependency inventory;
- `SHA256SUMS`, covering both files.

The same files are retained as a workflow artifact for 14 days. Only the final
publish job receives `contents: write`; the verification job additionally has
read-only deployment access, and all build jobs have read-only repository
access. The workflow uses the built-in `GITHUB_TOKEN` and needs no personal
access token. Repository or organization policy must permit release creation.

Do not create the tag directly. After updating the release metadata, building
the guide, committing and pushing `main`, wait for the Documentation workflow to
deploy GitHub Pages for that exact commit. Then run:

```sh
scripts/release-github.sh
```

The launcher requires a clean `main`, verifies that HEAD is on `origin/main`,
checks Cargo and AppStream versions, requires a successful `github-pages`
deployment for that exact commit, and refuses to reuse a local or remote tag.
The tag workflow repeats those checks so a manual tag push cannot bypass them.
See the [release checklist](../docs/release-checklist.md) for the complete
preparation sequence.

After the documentation and application gates pass, the release workflow stages
the assets on a draft and then publishes it automatically. This keeps a failed
upload from becoming a visible partial release. A rerun may replace assets on an
existing draft, but the workflow refuses to modify an already-published release.
Versions with a semantic prerelease suffix, such as `0.2.0-rc.1`, are published
as GitHub prereleases; build metadata after `+` does not make a stable version a
prerelease. Published notes link to the online guide and the exact tagged source
revision. Use a new version and tag after publication.

The archive is not an AppImage or a static binary. It requires a compatible
Linux userspace, a Wayland session and Vulkan drivers. Ubuntu 24.04 is the build
baseline; compatibility with older distributions is not established by the
workflow. A headless Actions build also does not replace native compositor,
accessibility and sustained-performance qualification.

## Installing an extracted archive

Run `bin/tachyon` directly from the extracted archive, or install its
contents under a chosen prefix. For a per-user installation:

```sh
mkdir -p "$HOME/.local/bin" "$HOME/.local/share"
cp -a tachyon-v0.1.0-linux-x86_64/bin/. "$HOME/.local/bin/"
cp -a tachyon-v0.1.0-linux-x86_64/share/. "$HOME/.local/share/"
update-desktop-database "$HOME/.local/share/applications"
```

Ensure `$HOME/.local/bin` is in the desktop session's `PATH`, since the desktop
entry launches `tachyon` by name. Substitute the downloaded version
for `v0.1.0`.
