# Table reading measures

A wide comparison keeps its authored column widths while long descriptions remain comfortable to read.

<!-- tachyon-table:v1 {"border":"LogicalPixel","widths":[240,1200]} -->
| Component | Appearance and behavior |
| --- | --- |
| Paragraphs and headings | One shared text engine. Formatting changes preserve the selection and scroll anchor. Heading changes immediately update the outline. |
| Bold, italic, strikethrough, inline code | Available through selection toolbar, context menu, and shortcuts. Mixed selections show mixed formatting state. |
| Lists | Hanging markers and 24 px indentation per level. Enter splits items; Enter on an empty item exits or outdents. Tab/Shift+Tab indent/outdent. Backspace at an item's start outdents before merging. |
| Task lists | Small native-style checkboxes aligned to the first text line. Toggling is one undoable edit. |
| Links | Underlined green text. Ordinary click positions the caret; Ctrl+click follows the link. A local popover edits label and destination. |
| Blockquotes | Indented text with a faint background tint and a narrow content-level rule. No surrounding box. |
| Code blocks | Quiet tinted background, monospace text, preserved whitespace, and local horizontal scrolling. No line numbers or editor chrome. |
| Images | Preserve aspect ratio, reserve space, and expose source/alt-text controls on selection. Loading and failure states occupy the same reserved area. |
| Tables | Body typography, semibold header, 12 px horizontal and 8 px vertical cell padding. Border modes: none, dotted, or one physical pixel; default dotted. No zebra striping. |

## Following section

Every description and table cell remains editable.
