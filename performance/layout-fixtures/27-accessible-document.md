# Accessible document

Synthetic content for native screen-reader traversal and keyboard verification.

## 1. Property comparison

### Connection properties

| Property | Value |
| --- | --- |
| Protocol | Local |
| Timeout | 30 seconds |

### Execution options

| Option | Enabled | Description |
| --- | --- | --- |
| Verify | Yes | Keep source content unchanged |
| Preview | Yes | Show the original content |

## 2. Task actions

- [ ] Unchecked native task
- [x] Completed native task

## 3. Source-order list

- **North:** First independent marker.
- **South:** Second independent marker.
- **East:** Third independent marker.
- **West:** Fourth independent marker.
- **Above:** Fifth independent marker.
- **Below:** Sixth independent marker.

## 4. Code and links

Read the [local heading](#7-final-offscreen-heading) before continuing.

```rust
let accessible_marker = 27;
```

## 5. HTML disclosure

<details><summary>Native disclosure summary</summary><p>Hidden authored body marker.</p></details>

## 6. Formula

$$\frac{37}{41}$$

This paragraph keeps the final heading below the initial viewport without hiding any primary document content. A screen-reader user must be able to reach the original content in canonical order, independent of which paragraphs currently appear on the physical display.

This second paragraph extends the reading surface. Changing the viewport or choosing an adaptive composition must not create a second primary copy of any heading, paragraph, list item, table cell, formula, link, or authored disclosure body.

This third paragraph keeps source-order traversal explicit. The visual renderer may reuse measured geometry, but a user navigating the document through accessibility APIs must not lose access to content outside the current painting window.

This fourth paragraph separates the final marker from the initial tables. Reading, selection, keyboard focus, and edit history remain distinct. Layout does not summarize or rewrite this content.

## 7. Final offscreen heading

Final canonical paragraph marker.
