# Edit Markdown

Click rendered text to place the caret, drag to select, and type as you would in
a text editor. Tachyon applies edits to a structured document and serializes the
result back to Markdown. Undo and redo operate on content changes; navigation,
search, zoom, and disclosure reading state do not enter content history.

## Format text and blocks

Select text or place the caret, then use the formatting toolbar or a shortcut:

| Result | Shortcut |
| --- | --- |
| Bold | <kbd>Ctrl</kbd>+<kbd>B</kbd> |
| Italic | <kbd>Ctrl</kbd>+<kbd>I</kbd> |
| Strikethrough | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>X</kbd> |
| Inline code | <kbd>Ctrl</kbd>+<kbd>E</kbd> |
| Link | <kbd>Ctrl</kbd>+<kbd>K</kbd> |
| Paragraph | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>0</kbd> |
| Heading 1, 2, or 3 | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>1</kbd>, <kbd>2</kbd>, or <kbd>3</kbd> |
| Ordered list | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>4</kbd> |
| Bulleted list | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>5</kbd> |
| Block quote | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>Q</kbd> |

Press <kbd>Enter</kbd> to split the current block. Press
<kbd>Shift</kbd>+<kbd>Enter</kbd> for an explicit Markdown hard break. Soft line
breaks from the source can reflow with the available width; hard breaks remain.

Task list items such as `- [ ] Review` render with interactive checkboxes. Select
a checkbox to change its authored state. In a list, <kbd>Tab</kbd> indents the
current item and <kbd>Shift</kbd>+<kbd>Tab</kbd> outdents it when the structure
allows the change.

## Copy and paste

Copy offers plain text, Markdown (`text/markdown`), and rich HTML on Wayland so
the receiving application can choose a format. Normal paste uses Tachyon's rich
Markdown clipboard data when it is available and otherwise inserts plain text.
Use <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>V</kbd> for **Paste as Markdown** when
you want pasted Markdown syntax to become document structure.

## Edit tables

Click a table cell and edit its text directly. Table commands preserve a valid
Markdown table while changing structure:

- <kbd>Tab</kbd> moves to the next cell. From the final cell it appends a row.
- <kbd>Shift</kbd>+<kbd>Tab</kbd> moves to the previous cell.
- <kbd>Ctrl</kbd>+<kbd>Enter</kbd> inserts a paragraph after the table and moves
  the caret there.
- Hover a cell edge to reach its edge control. The associated menu can insert a
  row above or below and insert a column left or right. At the top edge of a
  header cell it can delete that cell's column; at the left edge of a first-column
  cell it can delete that cell's row. Deletion is disabled when it would remove
  the table's last row or column.
- Dragging table boundaries can author column widths. Tachyon stores those
  widths in a `tachyon-table:v1` HTML comment; `null` leaves a column automatic.

Wide tables retain aligned columns and use contained horizontal overflow. While
a table is focused, its column widths stay stable as you type. Leaving the table,
resizing the window, or changing zoom allows Tachyon to measure the columns
again. Some two-column property tables and entity tables can receive a responsive
record presentation; source order and table data remain unchanged.

## Create and open links

Use <kbd>Ctrl</kbd>+<kbd>K</kbd> to add or edit a link. Press
<kbd>Alt</kbd>+<kbd>Enter</kbd> at a link to open it. Heading fragments and local
Markdown files stay inside Tachyon; web and mail links use the desktop handler.
See [Follow links](first-use.md#follow-links) for routing and safety rules.

Linked images, authored alt text, titles, and image paths survive editing,
copying, and undo. Normal Markdown HTTP(S) images use Tachyon's bounded loader
and disk cache. Keep local images beside the document when the file needs to be
portable offline.

## Edit supported HTML

Compatible HTML fragments render through an inert preview. You can select and
copy their text. Links and disclosure controls remain usable without rewriting
the source.

When Tachyon can verify a text leaf, click it for a temporary caret. The first
committed edit converts the fragment to Markdown and performs the edit in one
undoable transaction. Bold, italic, links, and all supported disclosure text are
retained in source order. One undo restores the original HTML after that direct
edit.

Conversion replaces authored HTML layout, styling, and disclosure controls with
Markdown content. Closed disclosures, missing summaries, and ambiguous previews
therefore offer an explicit **Edit text** conversion when Tachyon can preserve
the complete supported content. Unsupported or ambiguous structures remain as
source instead of accepting a lossy edit.

Useful limits to keep in mind:

- A supported open disclosure with a complete text map can be edited directly.
- A selection spanning multiple converted HTML text blocks cannot start one IME
  composition; use **Edit text** and choose one block.
- Image-only supported HTML figures can convert to Markdown figures while
  retaining links, titles, and alt text.
- Missing, oversized, animated, or remote HTML images retain a source fallback.
  CSS resource URLs are denied.
- Arbitrary active DOM behavior, scripts, network providers, and arbitrary CSS
  reproduction are outside the preview model.
