# Keyboard reference and troubleshooting

## Keyboard reference

| Task | Shortcut |
| --- | --- |
| New document | <kbd>Ctrl</kbd>+<kbd>N</kbd> |
| Open file / open folder | <kbd>Ctrl</kbd>+<kbd>O</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>O</kbd> |
| Save / Save as | <kbd>Ctrl</kbd>+<kbd>S</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>S</kbd> |
| Save a copy | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>S</kbd> |
| Export paged HTML | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>E</kbd> |
| Undo / redo | <kbd>Ctrl</kbd>+<kbd>Z</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd> |
| Toggle navigation | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>N</kbd> |
| Close window | <kbd>Ctrl</kbd>+<kbd>W</kbd> |
| Find | <kbd>Ctrl</kbd>+<kbd>F</kbd> |
| Next / previous result | <kbd>F3</kbd> / <kbd>Shift</kbd>+<kbd>F3</kbd> |
| Next / previous result while typing in Find | <kbd>Enter</kbd> / <kbd>Shift</kbd>+<kbd>Enter</kbd> |
| Close Find | <kbd>Escape</kbd> |
| Zoom in / out / reset | <kbd>Ctrl</kbd>+<kbd>=</kbd> / <kbd>Ctrl</kbd>+<kbd>-</kbd> / <kbd>Ctrl</kbd>+<kbd>0</kbd> |
| Bold / italic | <kbd>Ctrl</kbd>+<kbd>B</kbd> / <kbd>Ctrl</kbd>+<kbd>I</kbd> |
| Strikethrough / inline code | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>X</kbd> / <kbd>Ctrl</kbd>+<kbd>E</kbd> |
| Add or edit link / open link at caret | <kbd>Ctrl</kbd>+<kbd>K</kbd> / <kbd>Alt</kbd>+<kbd>Enter</kbd> |
| Paragraph / heading 1–3 | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>0</kbd> / <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>1</kbd>–<kbd>3</kbd> |
| Ordered / bulleted list | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>4</kbd> / <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>5</kbd> |
| Block quote | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>Q</kbd> |
| Hard line break | <kbd>Shift</kbd>+<kbd>Enter</kbd> |
| Copy / cut / paste | <kbd>Ctrl</kbd>+<kbd>C</kbd> / <kbd>Ctrl</kbd>+<kbd>X</kbd> / <kbd>Ctrl</kbd>+<kbd>V</kbd> |
| Paste as Markdown | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>V</kbd> |
| Select all | <kbd>Ctrl</kbd>+<kbd>A</kbd> |
| Move by word | <kbd>Ctrl</kbd>+<kbd>Left</kbd> / <kbd>Ctrl</kbd>+<kbd>Right</kbd> |
| Start / end of line | <kbd>Home</kbd> / <kbd>End</kbd> |
| Previous / next table cell | <kbd>Shift</kbd>+<kbd>Tab</kbd> / <kbd>Tab</kbd> |
| Exit table into a new paragraph | <kbd>Ctrl</kbd>+<kbd>Enter</kbd> |
| Page up / down | <kbd>Page Up</kbd> / <kbd>Page Down</kbd> |
| Extend selection by a page | <kbd>Shift</kbd>+<kbd>Page Up</kbd> / <kbd>Shift</kbd>+<kbd>Page Down</kbd> |
| Document start / end | <kbd>Ctrl</kbd>+<kbd>Home</kbd> / <kbd>Ctrl</kbd>+<kbd>End</kbd> |
| Extend selection | Add <kbd>Shift</kbd> to arrows, word, line, page, or document movement |
| Activate a focused disclosure or table-edge control | <kbd>Enter</kbd> or <kbd>Space</kbd> |

<kbd>Tab</kbd> is contextual: in a table it moves between cells, in a list it
changes indentation, and elsewhere it follows the native control order.
Inside an overflowing formula control, Left/Right pan, Home/End reach an edge,
Enter edits the source, and Escape returns to the document.

## Tachyon does not open a window

Release builds require a Wayland session, a Vulkan-capable graphics stack, and
compatible shared libraries. Confirm `XDG_SESSION_TYPE=wayland`, check the
terminal error, and compare missing libraries with the release's
`runtime-libraries.txt`. Compatibility with distributions older than the Ubuntu
24.04 build baseline is not established.

## A Markdown file is missing from Browser

The file browser lists `.md` and `.markdown` files plus directories. Other file
types are intentionally omitted. Check the extension and the selected browser
root. Use <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>O</kbd> to choose the intended
folder.

## My layout stays stacked

Widen the window or reduce zoom, finish editing the group, and wait for reflow to
settle. Then compare the source with the exact label, heading level, blank lines,
indentation, adjacency, and image requirements in the
[cookbook](adaptive-layouts.md#when-the-expected-arrangement-does-not-appear).
Long or uneven content may correctly stay vertical. There is no layout selector
that forces columns.

## An image or HTML preview shows source instead

For local images, check that the path is relative to the Markdown file and the
file is readable. Missing dimensions keep galleries stacked. Remote, missing,
oversized, or animated HTML images retain a source fallback, and CSS resource
URLs are denied. Arbitrary HTML/CSS is outside the supported inert-preview
subset.

## I cannot edit an HTML search result

Some HTML results have no verified editable text target. They remain selectable
and copyable. Use **Edit text** when offered, or edit a supported source fallback.
Click normal document text before typing if search left a read-only result active.

## Saving paused after another program edited the file

This protects both versions. Choose **Reload** to accept disk content, **Save
copy** to keep both, or **Overwrite** to replace the disk file with Tachyon's
version. See [Resolve an external file change](saving-and-recovery.md#resolve-an-external-file-change).

## A recovered draft appeared

Tachyon found an unresolved recovery snapshot from an earlier session. Restore it
to inspect and save that content, or dismiss it to keep the disk version or empty
draft. Restoring does not silently overwrite the named file.

## Text, emoji, or input composition behaves differently on my desktop

Tachyon uses installed fonts for scripts outside its bundled primary fonts and
relies on the Wayland compositor and input-method stack. Restart after changing
fonts. For an input-method problem, include the compositor, Fcitx5 or IBus
version, scale, script, and whether the issue occurs in Markdown text or a
converted HTML leaf when reporting it.
