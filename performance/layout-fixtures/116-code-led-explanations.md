## Retained settings

```toml
[workspace]
name = "field-notes"
storage = "local"

[editor]
appearance = "system"
autosave = true

[navigation]
files = true
outline = true

[reading]
zoom = 100
```

These settings belong to one local workspace. The name identifies the folder
shown in the navigator, while the storage field records that documents remain
on disk. Opening a different workspace does not copy or merge its files into
the current folder.

The editor follows the selected system appearance. Autosave writes committed
document changes without waiting for a separate save command. Both navigation
sections remain available, and the reading preference starts at the authored
zoom level. Changing the view must not rewrite the document content.

## Review the saved document

The complete example and its explanation remain above this heading.
