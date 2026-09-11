# A practical setup guide

Configuration values and their code example belong together. Give each section enough room to read, while keeping the document compact and easy to edit.

## Configuration options

The configuration keeps the document local and the presentation adjustable.

| Setting | Value | Purpose |
| --- | --- | --- |
| appearance | system | Follow the desktop |
| layout | auto | Use the available width |
| history | local | Keep changes on this computer |
| format | markdown | Preserve portable files |

## Configuration file

Save these values in your project configuration before opening the document.

```toml
[document]
appearance = "system"
layout = "auto"
history = "local"
format = "markdown"
autosave = true
```

## Notes for the team

This is an ordinary prose section, not another technical panel. Its heading should remain attached to the explanation, below the complete configuration pair. Narrow windows should put the configuration options first and the configuration file second, without changing the Markdown or the order of copied text.

## A separate chapter

### Installation options

| Method | Command |
| --- | --- |
| Local | cargo build |
| Check | cargo test |

### Environment file

```sh
EDITOR=mineral-markdown
PAGER=less
LANG=en_US.UTF-8
```
