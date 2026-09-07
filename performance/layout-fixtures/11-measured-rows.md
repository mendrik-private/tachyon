# Measured document rows

This synthetic document exercises source-order composition using native text measurements. It is not a summary of another document.

## Starting a worker

Read the saved configuration before starting the worker.

```rust
let configuration = read_configuration_from_workspace(&workspace_directory)?;
start_worker(configuration);
```

The next paragraph belongs below the complete explanation and code example, regardless of which side is taller.

## Review checks

### Content fidelity

Keep every authored block in its original reading order. Layout changes do not create content edits.

### Readable measures

Use actual font metrics to choose widths. Stack the sections when the available viewport becomes too short.

### Stable interaction

Keep the selected paragraph in place while typing. Undo restores the original Markdown bytes.

## Another example

The short example can share a row with its explanation when both have a comfortable width.

```sh
mineral-markdown document.md
```

---

### A separate section

The thematic break prevents this section from being pulled into the preceding group.
