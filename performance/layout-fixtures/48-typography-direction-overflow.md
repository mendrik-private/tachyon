# Typography, direction, and bounded overflow

Automatic layout must respond to the typeface that actually reaches the screen while the authored Markdown, semantic order, and reading position remain stable.

## 1. Measured body copy

Minimum viable typography mixes narrow and wide shapes: illuminating rivers, minimum widths, WWW interfaces, quiet revisions, interoperable workspaces, and deliberately repeated measurements. A fallback font must cause a fresh measurement rather than inheriting geometry from the previous face.

The second paragraph is long enough to wrap at ordinary widths and at 200% text size. It gives the native validation harness observable line geometry without depending on a screenshot hash, a specific compositor decoration, or an unbounded document.

## 2. Reading anchor

Keep this heading at the same visible position while the body font changes. The renderer may spend additional time measuring at startup, but a committed automatic layout must preserve the reader's anchor and must not oscillate after the same environment is applied twice.

### Stable identities

Headings retain stable semantic identities through remeasurement so outline navigation, assistive technology, selection, and editing continue to refer to the same document nodes.

## 3. Mixed direction and scoped overflow

مرحبا بالعالم — يحافظ التخطيط التلقائي على ترتيب المصدر وموضع القراءة عند تغيير الخط.

שלום עולם — הפריסה האוטומטית שומרת על סדר המקור ועל מיקום הקריאה בזמן שינוי הגופן.

An inline identifier such as `com.example.typography.direction.measurement.supercalifragilistic_identifier_without_break_opportunities_0123456789` may need local horizontal overflow; it must never widen or clip the entire page.

| Property | Deliberately unbroken value | Expected behavior |
| --- | --- | --- |
| Cache key | `org.example.product.rendering.typography.body_font_revision_0123456789abcdef` | Remeasure the affected group |
| Resource | `https://example.invalid/assets/typography-direction-overflow-validation-artifact-0123456789abcdef` | Contain overflow locally |

```text
UNBROKEN_VALIDATION_TOKEN_ABCDEFGHIJKLMNOPQRSTUVWXYZ_0123456789_abcdefghijklmnopqrstuvwxyz
```

## 4. Closing section

The final section makes the anchor meaningful in both directions: content exists above and below it, and no test needs to infer success from a document that cannot scroll.
