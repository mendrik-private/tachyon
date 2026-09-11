# Linux packaging

The desktop identity is `dev.mineral.Markdown`. A distributor should install:

- the `mineral-markdown` release binary under `bin/`;
- `dev.mineral.Markdown.desktop` under `share/applications/`;
- `dev.mineral.Markdown.metainfo.xml` under `share/metainfo/`; and
- the icon tree under `share/icons/hicolor/`; and
- the MIT and Apache-2.0 license texts under `share/licenses/mineral-markdown/`.

The binary is Wayland-only by product contract. The bundled fonts and their
licenses are compiled into the executable from `assets/fonts/`.

HTML previews keep the bundled Spline fonts as their default and use installed
fonts through Fontconfig for additional scripts and emoji. Distributions need
fonts covering the languages they support; no font files are downloaded from
documents. The native multilingual validation uses installed Noto CJK, Arabic
and Color Emoji fonts. After installing or changing fonts, restart Mineral to
rebuild its process-local font collection and retained previews.

The application icon is the supplied lightning/Markdown PNG, preserved at its
original resolution. GNOME scales it to the requested launcher size. Its icon
name and desktop filename match the Wayland application ID.

Build and stage a complete installation under an explicit prefix:

```sh
cargo build --release --locked --bin mineral-markdown
packaging/install.sh /tmp/mineral-stage/usr
```

The installer rejects an empty prefix and `/`; it never selects a system or
user prefix implicitly. Validate the staged desktop integration with:

```sh
desktop-file-validate /tmp/mineral-stage/usr/share/applications/dev.mineral.Markdown.desktop
appstreamcli validate --no-net /tmp/mineral-stage/usr/share/metainfo/dev.mineral.Markdown.metainfo.xml
file /tmp/mineral-stage/usr/share/icons/hicolor/scalable/apps/dev.mineral.Markdown.png
```

AppStream currently reports the optional `url-homepage-missing` warning. No
public project homepage has been chosen, so the metadata intentionally avoids
publishing an invented URL.

## GitHub release workflow

[Release](../.github/workflows/release.yml) builds a Linux x86_64 archive on
Ubuntu 24.04. A pushed `v*` tag starts the workflow. The tag must exactly match
`v` plus the version in `crates/markdown-app/Cargo.toml`, including any prerelease
suffix. Manual runs must select an existing matching tag, not a branch.

Before tagging, commit the complete application and its build inputs: manifests,
lockfiles, vendored sources, bundled fonts, test fixtures, documentation and
packaging assets. GitHub builds only the tagged commit; local untracked files
are not available to the runner. The Rust toolchain pinned in
`rust-toolchain.toml` must be downloadable on the runner.

The workflow runs `scripts/check.sh`, builds the optimized binary with `--locked`,
stages it with `packaging/install.sh`, validates the desktop entry and AppStream
XML, and checks for unresolved shared libraries. The resulting draft contains:

- `mineral-vVERSION-linux-x86_64.tar.gz`, with `bin/`, desktop integration,
  licenses and user documentation;
- `runtime-libraries.txt`, the build host's shared-library dependency inventory;
- `SHA256SUMS`, covering both files.

The same files are retained as a workflow artifact for 14 days. Only the final
release job receives `contents: write`; the build has read-only repository
permissions. The workflow uses the built-in `GITHUB_TOKEN` and needs no personal
access token. Repository or organization policy must permit release creation.

For example, after updating the application version to `0.1.0` and committing
the intended release contents:

```sh
git tag -a v0.1.0 -m 'Mineral 0.1.0'
git push origin v0.1.0
```

Review the generated notes and assets, complete native Wayland qualification,
and publish the draft from GitHub Releases. Mark prerelease versions as
prereleases when publishing. This follows GitHub's
[draft release workflow](https://docs.github.com/en/repositories/releasing-projects-on-github/managing-releases-in-a-repository?tool=webui).
Rerunning a tag can replace draft assets; it refuses to overwrite a published
release. Use a new version and tag for subsequent releases.

The archive is not an AppImage or a static binary. It requires a compatible
Linux userspace, a Wayland session and Vulkan drivers. Ubuntu 24.04 is the build
baseline; compatibility with older distributions is not established by the
workflow. A headless Actions build also does not replace native compositor,
accessibility and sustained-performance qualification.

## Installing an extracted archive

Run `bin/mineral-markdown` directly from the extracted archive, or install its
contents under a chosen prefix. For a per-user installation:

```sh
mkdir -p "$HOME/.local/bin" "$HOME/.local/share"
cp -a mineral-v0.1.0-linux-x86_64/bin/. "$HOME/.local/bin/"
cp -a mineral-v0.1.0-linux-x86_64/share/. "$HOME/.local/share/"
update-desktop-database "$HOME/.local/share/applications"
```

Ensure `$HOME/.local/bin` is in the desktop session's `PATH`, since the desktop
entry launches `mineral-markdown` by name. Substitute the downloaded version
for `v0.1.0`.
