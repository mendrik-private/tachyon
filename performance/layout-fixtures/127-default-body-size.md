# Default reading size

This is one working document for exploring Mineral’s implemented layout families. Read it wide, narrow, and with enlarged text: the same Markdown can become an open list, a measured row, a readable pair, a compact record, or a vertical stack without losing its source order.

The ordinary paragraph keeps the existing reading face while matching the size of the opening paragraph. Larger text must reflow naturally when the window becomes narrow and must keep the caret aligned with the visible words.

## Shared body size

- Begin with a wide window and read each complete line
- Narrow the window to check the source order and comfortable spacing
- Increase the zoom to keep the same document easy to read

## Independent cards

- **Document core:** Keep source text and history together
- **Document view:** Measure each line before painting its contents
- **Application:** Open files and remember the reader position

## Technical detail

```text
Code keeps its intentional compact size
```
