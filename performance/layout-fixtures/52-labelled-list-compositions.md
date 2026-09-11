# Automatic compositions across a document

The same authored Markdown can become a calm editorial layout without adding presentation metadata.

## System boundaries

Three responsibilities form one system, but each remains independently scannable.

- **Document core:** Stable nodes, transactions, source-preserving import, selection, undo, and serialization.
- **Document view:** Native shaping, measured layout, hit testing, virtualization, tables, images, and accessible interaction.
- **Application:** The Wayland shell, local file navigation, autosave, recovery, image loading, and performance instrumentation.

## Working interfaces

The repeated technical labels nominate a specification grid rather than a loose run of bullets.

- `DocumentSnapshot`: immutable content passed from the document core into the view.
- `LayoutPlan`: measured rows and cards selected for the available canvas.
- `SelectionState`: one stable source selection shared by editing and rendered geometry.
- `ResourceMap`: decoded image sizes and other external presentation inputs.
- `EditTransaction`: one reversible source mutation with a stable history boundary.
- `ViewportState`: scroll, zoom, and visible planning scope for the current window.

## Ordinary constraints

These points have deliberately uneven weight, so they should remain a spacious vertical list.

- Keep source order stable.
- Do not infer a comparison where none was authored; this longer point explains why semantic restraint matters more than filling every available cell in a wide window.
- Preserve the complete text and accessible reading order.

## Delivery sequence

1. Inspect the parsed document and its stable node identities.
2. Measure candidate arrangements with the native text renderer.
3. Reject any candidate that overflows or obscures relationships.
4. Publish the new geometry after editing becomes idle.
