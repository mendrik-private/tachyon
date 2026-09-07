# Accessible gallery

The first complete figure links to an authored heading below. The second figure is not a link. Their descriptions must remain available in source order in both paired and stacked layouts.

[![Linked landscape description](../visual-assets/mineral-strata.svg)](#gallery-destination "Read the destination")

![Unlinked landscape description](../visual-assets/mineral-strata.svg)

## Reading interval

This paragraph separates the gallery from its destination. It is ordinary authored content, not a generated caption, and must stay between the figures and the following heading.

The native accessibility check should activate the link that surrounds the first image. It must not activate a duplicate text control or rely on pointer coordinates.

The image keeps its accessible description whether the two figures share a row or stack. Neither arrangement changes the original Markdown or adds a content undo step.

The second image is intentionally unlinked. Its accessible role must not acquire a click action just because the adjacent figure has one.

The destination remains part of the same document. This synthetic test performs no external navigation and requires no network resources.

## Gallery destination

Destination marker for native image-link activation.

The following prose leaves enough room for the destination heading to move into the viewport. Selection and the source remain canonical after this navigation.

One final paragraph follows the destination in source order. No figure description is promoted into a visible caption.
