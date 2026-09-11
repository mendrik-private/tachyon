# Keep the configuration together

Read the available settings beside the complete example. Each section includes its own explanation after the technical content.

## Configuration options

These values control how local documents are opened and retained.

| Setting | Value | Purpose |
| --- | --- | --- |
| appearance | system | Follow the desktop |
| layout | auto | Use the available width |
| history | local | Keep changes on this computer |
| format | markdown | Preserve portable files |

The history setting keeps earlier revisions on this computer. It does not send the document to another service.

## Configuration file

Save the same values in the project configuration before opening a document.

```toml
[document]
appearance = "system"
layout = "auto"
history = "local"
format = "markdown"
autosave = true
```

The autosave option writes completed changes to the local file. Keep the original file when sharing an example with the team.

## Review the result

Open a document and confirm that its contents and earlier revisions are still available. This separate section follows both complete configuration entries.
