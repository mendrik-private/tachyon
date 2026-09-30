# Save, recover, and handle conflicts

Tachyon keeps Markdown on disk and maintains recovery state separately. Saving,
copying, recovery, and export have different effects, so choose the command that
matches what you want to preserve.

## Save, Save as, and Save a copy

- **Save** (<kbd>Ctrl</kbd>+<kbd>S</kbd>) writes the current named file. For an
  untitled document it opens the Save as chooser.
- **Save as** (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>S</kbd>) writes the document
  to a new path and makes that path the current document.
- **Save a copy** (<kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>S</kbd>)
  writes a separate Markdown file while the current document and its save target
  stay unchanged.

Named files autosave after 750 ms without another edit. Tachyon writes a
same-directory temporary file, flushes it, and atomically replaces the target
while retaining its permissions. A system or filesystem can still report a
durability or cleanup warning after document content was written; read the notice
instead of assuming that every part of the save completed.

## Recover an unsaved draft

Tachyon writes recovery snapshots independently of normal file saves, including
for untitled documents. If an unresolved snapshot is available the next time the
document or workspace opens, Tachyon shows a notice with actions to restore or
dismiss it.

- **Restore** loads the recovered Markdown into the editor as unsaved content.
  Review it and save when ready.
- **Dismiss** keeps the opened disk version or empty draft and discards that
  recovery offer.

Recovery is a safety net rather than another version-control system. Saving the
current revision clears its recovery record; newer edits made while a save is in
flight receive their own snapshot.

Recovery and workspace state follow the XDG base-directory convention. The
default state contains `tachyon/recovery` and `tachyon/workspace.json`. Set
`XDG_STATE_HOME` to isolate those files for a test run. `XDG_CACHE_HOME` similarly
isolates cache data.

## Resolve an external file change

Tachyon watches a named file for changes made by another program. If there are no
unsaved Tachyon edits, a modified file reloads automatically. If both the disk
file and Tachyon have changed, autosave pauses and the notice offers:

- **Reload**: replace the unsaved Tachyon document with the current disk file.
- **Save copy**: preserve the Tachyon version at a different path and leave the
  externally changed file untouched.
- **Overwrite**: replace the disk file with the Tachyon version after explicitly
  accepting the conflict.

Use Save copy when you need both versions and are unsure which is authoritative.
If the source was renamed or deleted, Tachyon asks you to save a copy rather than
silently recreating or overwriting an unexpected path.

## Export paged HTML

Press <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>E</kbd> or choose **Export paged
HTML…**. Tachyon writes a standalone, inert `.html` document with embedded print
styles, an A4 portrait page rule, print margins, a running title, and page-number
rules for browsers or paged-HTML renderers that support them.

Export uses a snapshot and does not change or save the Markdown source. The
export path cannot be the Markdown source path, and Tachyon adds an `.html`
extension when necessary. A document that changes while the file chooser is open
is not exported. If editing continues after export work starts, the completion
notice tells you that the file contains an earlier revision.

The export preserves semantic headings, lists, tasks, tables, authored table
widths, code, figures, footnotes, and safe supported HTML. It removes active
script content and editor-only controls. It is an HTML/print export rather than a
pixel-identical copy of the adaptive editor; review the result in the browser or
renderer you intend to use.
