# HTML table boundaries

Each fragment retains all of its authored content, including text before and after a table.

## Wrapped table and prose

<div><p>Before wrapper marker.</p><table><tr><th>Wrapper field</th><th>Wrapper value</th></tr><tr><td>Channel</td><td>Stable</td></tr></table><p>After wrapper marker.</p></div>

## Table followed by prose

<table><tr><th>Local field</th><th>Local value</th></tr><tr><td>Scope</td><td>Private</td></tr></table>
<p>Following fragment marker.</p>

## Adjacent tables in one fragment

<table><tr><th>First field</th><th>First value</th></tr><tr><td>Order</td><td>Alpha</td></tr></table>
<table><tr><th>Second field</th><th>Second value</th></tr><tr><td>Next</td><td>Beta</td></tr></table>

Final paragraph after every HTML fragment.
