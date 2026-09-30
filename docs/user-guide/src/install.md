# Install on Linux

Tachyon release archives target **Linux x86_64 with Wayland**. They are built on
Ubuntu 24.04 and need compatible system libraries plus a Vulkan-capable graphics
stack. The archive is not a static binary and Tachyon does not support X11 as a
product target.

## Install a release archive

1. Open the [latest Tachyon release](https://github.com/mendrik-private/tachyon/releases/latest).
2. Download the Linux archive, `SHA256SUMS`, and `runtime-libraries.txt`.
3. In a terminal in the download directory, verify the checksums and extract the
   archive. Replace `VERSION` with the release version:

   ```sh
   sha256sum --check SHA256SUMS
   tar -xzf tachyon-vVERSION-linux-x86_64.tar.gz
   ```

4. Run Tachyon directly:

   ```sh
   ./tachyon-vVERSION-linux-x86_64/bin/tachyon path/to/document.md
   ```

`runtime-libraries.txt` records the shared libraries from the build host. It can
help diagnose a missing-library error, but it does not turn the archive into a
portable or static bundle.

## Add desktop integration

To install an extracted release for your user, copy its `bin` and `share`
contents under `~/.local`:

```sh
mkdir -p "$HOME/.local/bin" "$HOME/.local/share"
cp -a tachyon-vVERSION-linux-x86_64/bin/. "$HOME/.local/bin/"
cp -a tachyon-vVERSION-linux-x86_64/share/. "$HOME/.local/share/"
update-desktop-database "$HOME/.local/share/applications"
```

Make sure `~/.local/bin` is in the `PATH` inherited by your desktop session.
After that, you can launch Tachyon from the application menu or run:

```sh
tachyon path/to/document.md
```

If Tachyon reports a missing shared library or cannot create a window, first
confirm that the session is Wayland, Vulkan works for native applications, and
the distribution is compatible with the Ubuntu 24.04 build baseline.

## Build the current source

Building requires Rust 1.98 and the native development libraries used by GPUI.
On Ubuntu, install the documented packages, then use the checked-in lockfile:

```sh
sudo apt-get install build-essential clang cmake git libfontconfig-dev \
  libglib2.0-dev libssl-dev libvulkan1 libwayland-dev libx11-xcb-dev \
  libxkbcommon-x11-dev libzstd-dev pkg-config
cargo build --release --locked --bin tachyon
target/release/tachyon path/to/document.md
```

From a source checkout, `./install.sh` builds the current local files and replaces
the per-user installation under `~/.local`. It does not pull updates from Git.
Pass an explicit prefix as its only argument to install somewhere else.
