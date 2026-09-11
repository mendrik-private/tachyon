# Mineral

Mineral is a native, Wayland-only Markdown editor written in Rust with GPUI. It
keeps Markdown rendered while it is edited: there is no source pane, preview
mode, account, or cloud service.

The current implementation includes a native title bar, file and folder
choosers, a resizable file/outline navigator, adaptive document layouts, rich
tables and lists, links and images, undo/redo, atomic autosave, external-change
handling, and crash recovery. The current design uses a warm light theme, green
accents, and serif headings. Bundled Fraunces, Spline Sans, Spline Sans Mono, and Noto Sans
fonts make rendering independent of the host font set.

## Build and run

Mineral requires Linux with a Wayland compositor, a Vulkan-capable graphics
stack, Rust 1.98, and the native development libraries used by GPUI. On
Ubuntu, the relevant packages are:

```sh
sudo apt-get install build-essential clang cmake git libfontconfig-dev \
  libglib2.0-dev libssl-dev libvulkan1 libwayland-dev libx11-xcb-dev \
  libxkbcommon-x11-dev libzstd-dev pkg-config
```
