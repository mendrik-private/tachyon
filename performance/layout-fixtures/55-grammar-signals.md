# Cards & signals

Communicate meaning with clear, consistent components. Open prose stays on the page; important exceptions receive a restrained signal.

## Context where it matters

> [!NOTE]
> This provides helpful, general information without indicating a required action.

> [!TIP]
> This suggests a practical shortcut that can make the work easier.

> [!IMPORTANT]
> Preserve source order, labels, and relationships in every arrangement.

> [!WARNING]
> Check the destination before replacing an existing file.

> [!CAUTION]
> This operation cannot recover information that was never saved.

## Technical examples

### Configuration

Use explicit values and preserve the original syntax.

```yaml
app:
  name: document-editor
  version: 2.1.0
  environment: production
  enabled: true
```

### Run the project

Keep commands selectable and copy their exact source.

```sh
cargo build --release --locked
./target/release/mineral-markdown plan.md
```

## Compact properties

| Property | Type | Default |
| --- | --- | --- |
| name | string | document-editor |
| retries | integer | 3 |
| enabled | boolean | true |

## Authored states

| Document | Owner | Status |
| --- | --- | --- |
| Design grammar | Design | Approved |
| Release checklist | Engineering | In review |
| Compatibility note | Documentation | Informational |
| Legacy integration | Engineering | Deprecated |

<details>
<summary>Additional explanation</summary>
<p>This authored disclosure preserves its open state, title, and complete content.</p>
</details>
