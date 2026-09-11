# System appearance and mineral surfaces — 2026-09-09

Continues the user's visual/layout priority and Audit A15. This is not full
visual-system or audit completion.

## Changes

`sync_mineral_component_theme` previously forced `ThemeMode::Light` and the light
palette both at startup and on every OS appearance event. It now uses GPUI
Component's system-appearance synchronization, then applies the matching shared
Mineral palette to native controls and document surfaces. The existing observer
handles live changes. No user preference, desktop setting or new app override is
introduced.

Dark mode now exposes the existing layered navy surfaces, ivory serif headings,
cool body text, code/table outlines and semantic syntax colors. Light mode keeps
the established paper/sage grammar. Tips now use the success color rather than
the brand accent: green in both appearances, distinct from amber Warnings.

## Evidence

The pre-fix `theme-before-dark.png` capture remained light when the isolated OS
settings portal reported dark; the independent palette oracle failed with zero
dark-role pixels. The Tip regression also failed before the color correction.

Final captures use normal debug binary SHA-256
`0486ef8192ad6d15af2e256548b6506cd891f4f71b5ee13a6f8901c789be3883`.
No layout-validation appearance override is used. The harness supplies the real
Settings.Read/SettingChanged protocol on its owned private D-Bus session, driving
the actual GPUI appearance observer. It never changes the physical desktop's
preferences. Private-service tests verify startup, Read, switching, refusal of
unowned buses and name release on cleanup.

| Capture prefix in `layout-previews/` | Coverage |
| --- | --- |
| `theme-final-selected` | Fixture 79, 1440 × 1000; dark → light → dark with native text selection and floating toolbar |
| `theme-final-signals` | Fixture 55, 1440 × 1000; light → dark → light, callout shapes/colors and technical content |
| `theme-final-narrow-200` | Fixture 79, 600 × 1000, 200% text zoom; dark canvas, wrapped headings and body |

Screenshots were inspected. Every cycle stage passes the independent page,
panel, heading and body-pixel checks. Heading/paragraph/table/cell/editor geometry
and accessible identities remain unchanged, as do editor focus and source bytes.
The selected-text cycle also makes a fresh native Copy at each stage and requires
identical nonempty text. A clipboard sentinel prevents stale clipboard data from
passing this check. The native selected-text SHA-256 is
`2d5dea12098af768b77f173960434e85147c842d8b788ac495fb8ef38a27a038`.

The accessible editor does not currently expose a Text interface with caret
offsets. The harness does not invent those fields or claim caret-offset/IME
qualification; it checks focus and selection through the evidence actually
available. The broader accessibility work remains open.

`scripts/check.sh` and `git diff --check` pass. Five theme-harness tests and
eleven capture-harness tests pass. The Rust view suite remains at 413 passing
tests and two intentionally ignored native-font tests; existing signal-token
tests now also enforce green/different-from-warning dark Tips.

Reproduce the selected-text cycle:

```sh
cargo build --locked --bin mineral-markdown
python3 performance/capture-layout.py --fixture 79-technical-sections.md \
  --binary target/debug/mineral-markdown --width 1440 --height 1000 \
  --zoom-steps 0 --appearance dark --appearance-cycle \
  --select 261 343 593 343 --source-unchanged-check --atspi-active \
  --output performance/layout-previews/theme-final-selected.png
python3 -m unittest discover -s performance -p test_theme_check.py
```

## Still open

Full component/menu/minimap coverage, high contrast, reduced-motion transitions,
fractional display scales, real-desktop multi-window theme behavior and visual
approval are not established by this bounded fixture matrix. Layout height-only
resizing and continuous-resize/scroll checks also remain open, as do other audit
correctness and performance requirements. This debug build makes no release
performance claim.
