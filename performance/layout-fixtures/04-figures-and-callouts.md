# Figures with room to breathe

A pair of neighboring figures can share a row. The full image remains visible, and descriptive alt text stays an accessibility description.

## Related figures

![Layered green geological strata with document blocks](../visual-assets/mineral-strata.svg)

![Three layout candidates arranged from one column to three columns](layout-candidates.svg)

These captions are authored as an ordinary paragraph after the pair. The renderer does not invent a caption from either image's alternative text.

## A standalone illustration

![One, two and three-column arrangements represented with green rectangles](layout-candidates.svg)

## Semantic callouts

> [!NOTE]
> The document model retains image order and the associated alternative text.

> [!TIP]
> Use a narrow window to verify that the figure pair stacks without cropping.

> [!IMPORTANT]
> A screenshot and a diagram need their full frame. Containing an image is a safer default than cropping it.

> [!WARNING]
> A long paragraph between images breaks the pair because it may describe the first image.

> [!CAUTION]
> A layout must never silently hide critical content.

## Conservative HTML

<details open>
<summary>An authored disclosure</summary>
This source remains recoverable through the existing conservative HTML support.
</details>
