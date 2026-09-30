# Search a document

Press <kbd>Ctrl</kbd>+<kbd>F</kbd> or select the search button in the title bar.
Type a query, then use:

- <kbd>Enter</kbd> or <kbd>F3</kbd> for the next match;
- <kbd>Shift</kbd>+<kbd>Enter</kbd> or <kbd>Shift</kbd>+<kbd>F3</kbd> for the
  previous match;
- <kbd>Escape</kbd> to close search and return focus to the document.

Search follows Markdown source order. It includes supported HTML disclosure
text, even when a disclosure is currently closed. A match inside a wide table,
code block, or editable HTML fragment scrolls that local region into view without
shifting the whole page horizontally.

Searching never edits the Markdown and does not add an entry to content undo
history. Most results place an editable caret. A result inside HTML without a
verified editable text target can be selected and copied but remains read-only;
click editable document text before typing.

If a result seems absent, check that the query is exact and continue through all
matches. Search does not describe a case-sensitive or regular-expression mode,
so do not rely on either behavior as a query option.
