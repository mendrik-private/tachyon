# Visual fixture protocol

The canonical complete-component source is `performance/visual-fixture.md`. It covers the application shell, prose and inline styles, Unicode and bidirectional shaping, quotes, alerts, nested and task lists, code, wide content, tables, a local image, thematic breaks, footnotes, and preserved safe HTML.

The checked-in PNGs were captured from a 1280 × 900 Weston headless output with its GL renderer on the target Radeon 8060S GPU. A private test-only Weston protocol module advertised the exact `wp_fractional_scale_v1` preferred scale in protocol units (120, 150, 180, and 240). This matters: changing only an application-internal scale is not equivalent to compositor negotiation and is not accepted as evidence.

Build the application and the isolated compositor harness:

```sh
cargo build --release -p markdown-app --bin tachyon
performance/wayland-harness/build.sh
```

For each target, start the compositor with `MINERAL_WESTON_SCALE_120` set to the corresponding protocol value and load both generated modules:

```sh
MINERAL_WESTON_SCALE_120=150 weston --backend=headless --renderer=gl \
  --width=1280 --height=900 --socket=mineral-fixture --idle-time=0 \
  --debug --no-config --shell=kiosk --refresh-rate=120000 \
  --modules="$PWD/performance/wayland-harness/build/virtual-input.so,$PWD/performance/wayland-harness/build/fractional-scale.so"
WAYLAND_DISPLAY=mineral-fixture target/release/tachyon performance/visual-fixture.md
WAYLAND_DISPLAY=mineral-fixture weston-screenshooter
```

Store lossless output screenshots as `visual-scale-100.png`, `visual-scale-125.png`, `visual-scale-150.png`, and `visual-scale-200.png` beside this file. Use `performance/wayland-harness/build/input-client move X Y` followed by repeated `scroll 0 1000` calls to exercise the lower fixture. The module repeats the preferred scale after surface entry because Weston 14 advertises `wl_compositor` v5 and otherwise sends its integer output scale during the initial enter event.

For every fixture, verify that the title bar and controls are present, document and navigation text is not cropped, focus and selection remain visible, the minimap stays a document silhouette, the local image reserves its aspect ratio, and there are no gaps or overlaps at panel and table boundaries. The automated GPUI test renders this same source at all four scales and proves that shaped-line cache entries are keyed by the exact scale factor.
