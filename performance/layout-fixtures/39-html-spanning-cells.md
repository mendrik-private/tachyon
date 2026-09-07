# HTML table structure

Spanning cells retain their authored relationships. Layout must not flatten a shared heading or move a value into the wrong column.

## Column and row spans

<table>
<tr><th colspan="2">Release plan</th><th>Owner</th></tr>
<tr><td rowspan="2">Review</td><td>Draft</td><td align="right">Ada</td></tr>
<tr><td>Sign-off</td><td align="right">Lin</td></tr>
<tr><td>Ship</td><td>Publish</td><td align="right">Kai</td></tr>
</table>

## Fragment spacing and alignment

<div><p>Before the compact facts.</p><table><tr><th>Property</th><th>Value</th></tr><tr><td>Count</td><td align="right">17</td></tr><tr><td>Channel</td><td align="center">Stable</td></tr></table><p>After the compact facts.</p></div>

This final paragraph remains normally editable.
