# The shape of a record

A source-backed schema tree keeps the document's hierarchy visible, with aligned fields and values that remain readable beside the explanation.

## A retained message

This record keeps an identifier, an ordered list of recipients, and the sender's address. The schema is displayed structurally: its keywords remain visible, and the tree does not imply that a message has passed validation.

The required array is shown exactly where it appears in the source. Unknown annotations and reference destinations are retained without being fetched or interpreted. Click the tree to edit the original JSON; copying still uses the authored source.

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "Retained message",
  "type": "object",
  "properties": {
    "messageId": { "type": "string" },
    "recipients": {
      "type": "array",
      "items": { "type": "string" }
    },
    "sender": {
      "type": "object",
      "properties": {
        "address": { "type": "string" }
      }
    }
  },
  "required": ["messageId", "recipients"],
  "x-retention": 1.00e+5
}
```

## Ordinary JSON stays code

A configuration object without an explicit schema dialect retains its normal syntax-highlighted code pane. No layout heuristic turns these settings into a schema.

```json
{
  "type": "object",
  "properties": { "theme": "paper", "zoom": 100 }
}
```

## References are inert

The destination below is only a value in the source tree. The renderer makes no network request and reports no fabricated validation result.

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$ref": "https://example.invalid/never-fetch.json",
  "examples": [null, true, false, "<tag>& text", {}]
}
```
