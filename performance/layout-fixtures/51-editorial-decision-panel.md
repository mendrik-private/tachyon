# Native Markdown viewer and rich editor

## 1. Product contract

Build a native Rust/GPUI application for **Linux Wayland**, with one continuously rendered, always-editable Markdown surface. Clicking places the caret; selecting text reveals formatting tools. Raw Markdown is never exposed.

The first release includes local file navigation, an outline, adaptive document layouts, rich tables, lists, links, images, code examples, undo, and autosave. The redesign targets **scrolling above 60 fps through 10 MB**. Startup preparation may take longer.

Confirmed decisions:

- Full rich content inside table cells.
- Code blocks remain rendered and editable document content.
- Autosave preserves untouched Markdown regions byte-for-byte.
- Remove the minimap for now; retain Files and Outline.
- Use warm paper, dark serif headings, green accents, fine borders, and restrained panels.
- Start with the light theme, independent of the system appearance.
- Choose layout automatically while keeping editing and source order stable.
- No tabs, source pane, accounts, synchronization, or plugin system in v1.

## 2. Visual and component system

### Typography

| Role | Face | Size / leading |
| --- | --- | ---: |
| Body | Spline Sans | 18 / 28.8 |
| H1 | Fraunces 600 | 44 / 50 |
| H2 | Fraunces 600 | 28 / 34 |
| Code | Spline Sans Mono | 15 / 22.5 |

### Palette

| Role | Value |
| --- | --- |
| Page / navigation | `#FCFBF8` / `#F3F2ED` |
| Text / secondary | `#1B2430` / `#59636F` |
| Accent / selection | `#256F50` / `#DCEBE1` |
| Rules / errors | `#DEDFD7` / `#9E4B3F` |

### Window anatomy

| Region | Behavior |
| --- | --- |
| Files | Resizable navigation and local documents |
| Canvas | Centered prose with elastic components |
| Margin | Optional notes fold into the flow |
