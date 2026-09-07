# Rich cell panels

Code and authored disclosures retain their own presentation inside a table. Their neighbors are independent cells.

## Code cells

<table>
<thead><tr><th>Example</th><th>Explanation</th></tr></thead>
<tbody>
<tr><td><pre><code>let ready = true;
if ready {
    run_job();
}</code></pre></td><td><p>Code neighbor remains readable.</p></td></tr>
<tr><td><pre><code>connection_with_a_deliberately_long_identifier = open_connection_with_explicit_options();</code></pre></td><td><p>Long code neighbor.</p></td></tr>
</tbody>
</table>

This paragraph follows the code table.

## Authored cell disclosures

<table>
<thead><tr><th>Authored content</th><th>Neighbor</th></tr></thead>
<tbody>
<tr><td><details open><summary>Open cell details</summary><p>Visible cell body with <strong>strong text</strong>.</p></details></td><td><p>Open details neighbor.</p></td></tr>
<tr><td><details><summary>Closed cell details</summary><p>Closed cell body marker.</p></details></td><td><p>Closed details neighbor.</p></td></tr>
</tbody>
</table>

Final paragraph after both tables.
