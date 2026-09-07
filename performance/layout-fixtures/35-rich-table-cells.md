# Rich table cell boundaries

HTML tables can carry authored blocks inside their cells. The quote, list, and quoted annotation below belong only to their own cell.

## Cell-local containers

<table>
<thead><tr><th>Rich content</th><th>Neighbor</th></tr></thead>
<tbody>
<tr><td><blockquote><p>Quoted cell text.</p></blockquote></td><td><p>Quote neighbor.</p></td></tr>
<tr><td><ul><li>First cell item</li><li>Second cell item</li></ul></td><td><p>List neighbor.</p></td></tr>
<tr><td><blockquote><p>[!NOTE]</p><p>Cell notice text.</p></blockquote></td><td><p>Notice neighbor.</p></td></tr>
<tr><td><ol start="9"><li>First numbered cell item</li><li>Second numbered cell item</li></ol></td><td><p>Numbered neighbor.</p></td></tr>
</tbody>
</table>

This paragraph belongs outside the table.

## Containers on both sides

> <table>
> <thead><tr><th>Plain content</th><th>Quoted content</th></tr></thead>
> <tbody><tr><td>Independent neighbor.</td><td><blockquote><p>Nested quote in a quoted table.</p></blockquote></td></tr></tbody>
> </table>

The final paragraph remains outside the quoted table.

## Ordered table ancestor

1. <table>
   <thead><tr><th>Nested item</th><th>Value</th></tr></thead>
   <tbody><tr><td><ol start="3"><li>Numbered inside and outside.</li></ol></td><td>Independent value.</td></tr></tbody>
   </table>
