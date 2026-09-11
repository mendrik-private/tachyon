# Configuration in context

The explanation and its technical evidence belong together. Available width should help a reader compare them without losing the thread.

## Retained document settings

The document keeps its reading preferences separate from the underlying text so that the same file remains useful in other editors. A setting records how this workspace presents the material, not a replacement for the material itself. The table describes the small set of retained values and their scope. Changes to these values can alter the view immediately, while opening the Markdown in another application still exposes the complete original paragraphs, links, tables and illustrations.

Values are applied when the workspace opens. Temporary selection and an active editing position remain owned by the current window rather than being written into the document settings.

| Setting | Scope | Retained value |
| --- | --- | --- |
| Text size | Workspace | Reader preference |
| Appearance | System | Light or dark |
| Navigation | Window | Files and outline |
| Source | Document | Original Markdown |
| History | Session | Editing commands |

## Applying a configuration

The worker reads a complete configuration before publishing a new view. It first checks that the requested text size and presentation mode can be represented, then prepares the resources needed by the document. Publication is a single operation so that the renderer never sees half of the new configuration alongside half of the old one. If preparation fails, the previous view stays available and the error describes which setting needs attention. The example preserves that order without changing any authored content.

```rust
fn apply_configuration(
    workspace: &Workspace,
    source: &Document,
) -> Result<PreparedView> {
    let settings = workspace.read_settings()?;
    settings.validate()?;
    let resources = prepare_resources(source)?;
    let view = prepare_view(source, settings, resources)?;
    workspace.publish(&view)?;
    Ok(view)
}
```

## Reading continues

The next section returns to ordinary document flow. Nothing has been shortened or moved across a section boundary to create the paired arrangements above.
