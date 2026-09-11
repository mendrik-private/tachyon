# Architecture and persistence

## Dependencies and ownership

Use three crates:

- **Document core:** structured content, selections, transactions, undo, Markdown/HTML import, and source-preserving serialization. No GPUI dependency.
- **Document view:** shaping, adaptive arrangements, hit testing, selection painting, tables, viewport virtualization, and outline projection.
- **Application:** GPUI window, commands, navigation, filesystem services, image cache, and recovery.

## Ordinary constraints

- Keep source order stable.
- A longer uneven point remains part of an ordinary list, keeping all of its explanation visible without forcing several independent cards into an imbalanced row.
- Preserve the complete text.
