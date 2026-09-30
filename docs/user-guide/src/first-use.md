# First use, files, and navigation

Launch Tachyon with a Markdown file path, a folder path, or no argument:

```sh
tachyon notes.md
tachyon path/to/project
tachyon
```

A file opens that document. A folder sets the Markdown browser root to that
folder; select the **Browser** tab to see its files. With no path, Tachyon restores
the last workspace when possible or opens a recoverable untitled draft.

## Create and open documents

- Press <kbd>Ctrl</kbd>+<kbd>N</kbd> for a new untitled document.
- Press <kbd>Ctrl</kbd>+<kbd>O</kbd> to open a file.
- Press <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>O</kbd> to choose a folder for the
  file browser.

The file chooser first tries the folder from which a file was last successfully
opened. It then falls back to the current document's folder or the visible
browser root. Canceling the chooser does not replace the remembered folder.

The Browser tab lists directories and files whose extension is `.md` or
`.markdown`, case-insensitively. Open a directory to expand it and select a file
to open it. When you opened an individual file instead of a folder, Tachyon uses
the file's parent folder for navigation unless you later choose an explicit
folder.

## Use Outline and Browser

Press <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>N</kbd> to show or hide navigation.
On a wide window the navigation panel sits beside the document. On a narrow
window it appears as an overlay so the document retains usable width.

The panel has two mutually exclusive tabs:

- **Browser** shows the current Markdown folder tree.
- **Outline** shows headings from the open document and lets you jump to one.

Select a tab with the pointer, or focus a tab and use Left/Up for Outline and
Right/Down for Browser. Enter and Space also activate the focused tab. Each tab
keeps its own scrolling behavior. Drag the panel's right edge with the pointer to
resize it. The document scrollbar stays at the outer window edge.

Tachyon remembers the active path, folder root, expanded folders, navigation
width, selection, and reading position in its workspace state. Restoring state
can fail when a file was moved, permissions changed, or the state directory is
unavailable; use Open file or Open folder to choose a new location.

## Follow links

Place the caret in a Markdown link and press <kbd>Alt</kbd>+<kbd>Enter</kbd>, or
hold <kbd>Ctrl</kbd> and click it. The context menu also offers link actions.
Tachyon routes:

- `http`, `https`, and `mailto` links to the desktop;
- heading anchors within the current document, including duplicate-heading
  suffixes;
- relative local `.md` and `.markdown` files;
- authored IDs in supported HTML previews.

Local file paths are resolved relative to the current document's folder.
Percent-encoded filenames and Unicode heading anchors are supported. Missing
files or headings produce an error without changing the Markdown. Other
protocols and remote file hosts are not opened. If the current document has
unsaved edits, Tachyon saves or asks how to resolve them before switching to a
linked local file.
