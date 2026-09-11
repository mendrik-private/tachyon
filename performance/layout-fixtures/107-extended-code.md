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

