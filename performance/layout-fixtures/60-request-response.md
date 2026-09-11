# An exchange, not two unrelated examples

Requests and responses share a measured row when both remain readable. The labels, methods, paths and outcomes below are authored content, not inferred results.

## Create a document

### Request: Create

`POST /v1/documents`

```json
{
  "title": "Field notes",
  "format": "markdown"
}
```

### Response: Created

`201 Created`

```json
{
  "id": "doc_field_notes",
  "title": "Field notes"
}
```

## Three adjacent objects

The complete pair stays together; a following unpaired request does not turn it into a three-card comparison.

### Request: Inspect

```http
GET /v1/documents/doc_field_notes
Accept: application/json
```

### Response: Found

```http
HTTP/1.1 200 OK
Content-Type: application/json
```

### Request: Delete

```http
DELETE /v1/documents/doc_field_notes
If-Match: "revision-4"
```

## A rejected request

### Request: Missing title

`POST /v1/documents`

```json
{ "format": "markdown" }
```

### Response: Rejected

`400 Bad Request`

```json
{ "error": "title is required" }
```

## Request handling

Ordinary narrative remains open. A heading containing the word request does not by itself describe an exchange object.

### Request

Please review the document before the next discussion. This prose-only request stays in the normal reading flow.
