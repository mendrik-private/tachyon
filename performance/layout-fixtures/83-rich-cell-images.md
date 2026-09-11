# Figures inside table cells

An image belongs to its column. Its aspect ratio, inset and neighboring text must remain stable as the window changes.

## Authored columns

<!-- tachyon-table:v1 {"border":"LogicalPixel","widths":[240,360]} -->
<table>
<thead><tr><th>Figure</th><th>Notes</th></tr></thead>
<tbody>
<tr><td><p><img src="gallery-1.svg" alt="First cell figure"></p><p>Caption inside the image cell.</p></td><td><p>The neighboring text stays inside its own column.</p></td></tr>
<tr><td><blockquote><p><img src="gallery-2.svg" alt="Inset cell figure"></p></blockquote></td><td><p>The quoted figure keeps its extra inset.</p></td></tr>
</tbody>
</table>

The paragraph after the table must follow the entire row, not overlap the image.
