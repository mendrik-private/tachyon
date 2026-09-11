# Local images inside HTML

This synthetic document checks loaded resources, source order, and conversion without changing the original HTML while reading.

<div style="padding:16px;border:1px solid #cdd4ce;border-radius:8px"><p>Six independent ideas can share a readable page while images keep their complete composition.</p><div style="display:flex;flex-wrap:wrap;gap:16px"><a href="02-list-arrangements.md"><img src="../visual-assets/tachyon-strata.svg" alt="Layered tachyon strata" title="Original composition" width="320" height="120"></a><img src="../visual-assets/tachyon-strata.svg" alt="Repeated source, second placement" width="320" height="120"></div><p><strong>A styled HTML fragment</strong> with a <a href="https://example.test/reference">reference link</a>.</p></div>

## After the figure

Both images have one primary location. Their alt text is an accessibility description, not a visible caption. Editing the text converts the fragment to Markdown; undo restores the exact authored HTML.

> Resource loading should preserve this reading position and must not enter content undo history.

## Missing resource fallback

<div><p>This paragraph and its missing figure must not silently disappear.</p><img src="missing-local-figure.png" alt="Unavailable synthetic figure"></div>

## Explicit remote fallback

<div><p>This remote reference is not permission for Blitz to fetch it.</p><img src="https://example.test/denied.png" alt="Denied remote figure"></div>
