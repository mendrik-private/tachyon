# Local images inside HTML

This synthetic document checks loaded resources, source order, and conversion without changing the original HTML while reading.

Six independent ideas can share a readable page while images keep their complete composition\.

![Layered tachyon strata](../visual-assets/tachyon-strata.svg "Original composition")![Repeated source\, second placement](../visual-assets/tachyon-strata.svg)

**A styled HTML fragment** with a [reference link](https://example.test/reference)\.





## After the figure

Both images have one primary location. Their alt text is an accessibility description, not a visible caption. Editing the text converts the fragment to Markdown; undo restores the exact authored HTML.

> Resource loading should preserve this reading position and must not enter content undo history.

## Missing resource fallback

<div><p>This paragraph and its missing figure must not silently disappear.</p><img src="missing-local-figure.png" alt="Unavailable synthetic figure"></div>

## Explicit remote fallback

<div><p>This remote reference is not permission for Blitz to fetch it.</p><img src="https://example.test/denied.png" alt="Denied remote figure"></div>
