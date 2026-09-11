# Choices, evidence & trade-offs

Named objects give a document useful structure. Their boundaries express an authored decision, a selected option or a worked example; ordinary explanations remain in the open reading flow.

## A deliberate choice

### Decision: Keep the source authoritative

Use one Markdown document for reading and editing. Presentation can change the arrangement without changing the author’s words.

### Selected option: Local files

Keep documents in the workspace folder. Autosave preserves the existing file and leaves recovery evidence when a write cannot complete.

## Considered trade-offs

### Pros

- **Readable:** Related ideas share clear alignment.
- **Stable:** Typing does not move the active card.
- **Portable:** Saved Markdown needs no layout annotations.

### Cons

- **Preparation:** Actual font measurement takes time.
- **Complexity:** Every arrangement needs a source mapping.
- **Coverage:** New patterns need visual and interaction tests.

## Worked examples

### Example: Read a document

Open the authored file without replacing its contents.

```rust
let source = read_to_string(path)?;
let document = parse(&source)?;
```

### Example: Save a document

Serialize only the changes the author made.

```rust
let source = document.serialize()?;
write_atomically(path, &source)?;
```

## Validation with explicit outcomes

### Valid: A named document

The required title and body are present.

```json
{"title": "Field notes", "body": "Rain at noon."}
```

### Invalid: The title is missing

Supply a title before publishing. The body is still available for editing.

```json
{"body": "Rain at noon."}
```

## Ordinary sections stay open

### Decision making takes context

This is an explanation, not a decision record. Its title alone does not assert an outcome. An attractive document should make that distinction visible without forcing every paragraph into a container.

Readers also need uninterrupted prose. Space between ideas matters as much as the objects themselves, and longer explanations should keep a comfortable reading measure rather than stretching across the whole window.
