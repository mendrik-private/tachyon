# Create a document

An API reference combines a short explanation, a compact parameter table, and exact code. Each part should receive the width it needs.

## Authentication

Include your development API key in the request headers. This synthetic endpoint demonstrates typography and layout; it does not contact a service.

> [!WARNING]
> Keep credentials out of source control and shared examples. All values in this fixture are placeholders.

## Parameters

| Name | Type | Required | Description |
| :--- | :--- | :---: | :--- |
| `title` | string | Yes | Human-readable document title |
| `content` | string | Yes | Markdown source, including deliberate whitespace |
| `folder_id` | string | No | Parent folder for the document |
| `tags` | array | No | Labels used to organize related documents |

## Request

```json
{
  "title": "A quieter place to think",
  "content": "# Field notes\n\nA short introduction.\n",
  "folder_id": "field-notes",
  "tags": ["writing", "design", "reference"]
}
```

## Configuration

| Property | Value |
| :--- | :--- |
| `appearance` | light |
| `layout` | auto |
| `prose_measure` | 70 characters |
| `scrolling` | more than 60 fps |

## Comparison across environments

| Environment | Region | Concurrent editors | History in days | Requests per month | Storage limit | Deployment approval |
| :--- | :--- | ---: | ---: | ---: | ---: | :--- |
| Local development | On this computer | 1 | 7 | 1,000 | 500 MB | Not required |
| Shared preview | Northern Europe | 8 | 30 | 10,000 | 5 GB | Project maintainer |
| Production | Northern Europe | 40 | 365 | 1,000,000 | 100 GB | Release owner |

## Exact code and horizontal scrolling

```rust
fn preserve_source(source: &str) -> String {
    // Whitespace and long lines remain exact when copied.
    let example = "a_deliberately_long_identifier_that_demonstrates_contained_horizontal_scrolling_without_shrinking_the_document_or_silently_wrapping_the_source";
    format!("{source}\n{example}")
}
```

## A sequence with an example

1. Create the configuration file in your project folder.

   ```yaml
   appearance: light
   document:
     layout: auto
   ```

2. Run the application with the document path.

   ```sh
   mineral-markdown notes.md
   ```

3. Verify that your folder and document outline remain available.
