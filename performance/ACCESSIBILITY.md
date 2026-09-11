# Accessibility verification

Mineral exposes its GPUI scene through AccessKit's Linux AT-SPI adapter. The
document surface is one multiline text input with text selection and caret
state. Its virtualized visible children carry structural roles rather than a
flat list of labels: headings, paragraphs, links, lists and list items, code,
images, tables, rows, column headers, cells, block quotes, alerts, notes, and
thematic-break separators. File navigation is a tree, the outline is a tree,
the Files/Outline divider is a slider, toolbar controls are buttons, and the
minimap exposes scrollbar semantics.

## Recorded native check

On 2026-09-06, the release binary was checked in a native Wayland session over
the session accessibility bus. The live AT-SPI tree contained 41 virtualized
nodes for `performance/visual-fixture.md`. Visible nodes had non-zero screen
bounds, the editor exposed text coordinates and selection, and headings, list
containers, and list items appeared as distinct roles. Sending a native `x`
key event to the focused text input changed the document and the 750 ms
autosave persisted it. This confirms that the observed tree belongs to the
interactive editor rather than a static duplicate.

The count is intentionally not stable: virtualization exposes the visible
document plus overscan and changes the child set as the viewport moves.

## Reproduction checklist

1. Run `cargo build --release --locked --bin tachyon` in a graphical
   Wayland login with the distribution's AT-SPI bus enabled.
2. Start `target/release/tachyon performance/visual-fixture.md`.
3. Inspect the application with Accerciser, `pyatspi`, or Orca. Confirm that the
   focused editor is a multiline editable text object and query its text,
   selection, caret, and extents interfaces.
4. Traverse the visible document and confirm that heading, link, list,
   list-item, image, table, row, header, and cell nodes have usable names and
   screen bounds.
5. Traverse both navigation trees, activate a file and an outline heading,
   adjust the split slider using the arrow keys, toggle navigation with
   `Ctrl+Alt+N`, and reach the title-bar menu without a pointer.
6. Type through IBus or Fcitx5, undo and redo, and verify that caret and
   selection announcements follow the resulting state.
7. Enable the desktop's reduced-motion preference and verify that outline
   jumps are immediate. Repeat at compositor scales 100%, 125%, 150%, and
   200%; bounds must continue to match painted content.

The native tree, role, bounds, keyboard-edit, and autosave checks above were
performed. Spoken Orca output, every IME engine, and every compositor scale are
manual release checks; this document does not claim an automated substitute
for them.
