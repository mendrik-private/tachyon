# Selecting technical text

Selections remain readable across prose, code and table cells. Copy keeps the original text and editing stays in the same layout.

## Executable sample

```rust
// Keep every syntax role readable
let value = (42, "label");
if value.0 > 0 {
    println!("retained text");
}
```

## Configuration sample

```json
{
    "enabled": true,
    "count": 42,
    "label": "retained value"
}
```

## Mixed reference cell

<table>
<thead><tr><th>Reference</th><th>Meaning</th></tr></thead>
<tbody><tr><td><p>Before the sample</p><pre><code class="language-rust">let retained = 42;</code></pre><p>After the sample</p></td><td><p>The complete cell includes ordinary text and an executable sample. Selection must preserve both surfaces.</p></td></tr></tbody>
</table>
