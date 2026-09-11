# Visual and component design system

## Typography and palette

The [reference site](https://www.whichai.dev/with-design-skill/opus-5/4) specifies **Fraunces** headings, **Spline Sans** body text, and **Spline Sans Mono** technical text. Its heading settings include weight 600, negative tracking, and Fraunces's `SOFT`, `WONK`, and optical-size axes.

Use those families and heading characteristics at document-appropriate sizes:

| Role | Typography |
| --- | --- |
| Body / lead / table cells | Spline Sans 18/28.8 px; lead 20/31 px; table 15.5/25 px |
| H1 | Fraunces 44/50 px, weight 600; `SOFT=30`, `WONK=1`, `opsz=120` |
| H2 | Fraunces 28/34 px, weight 600; `SOFT=40`, `WONK=1`, `opsz=72` |
| H3-H6 | Fraunces 26/22/19/17 px, weight 600, line height 1.2; `SOFT=40`, `WONK=1`, `opsz=20` |
| Navigation and controls | Spline Sans 13 px/1.4 |
| Code | Spline Sans Mono 15 px/1.5 |

Bundle fonts locally with their licenses. Generate static Fraunces instances during asset preparation: the inspected GPUI font interface does not expose arbitrary variation axes. Include deterministic italic/oblique faces and fallback coverage for unsupported scripts.

The following light palette is an adaptation for this application:

| Token | Value |
| --- | --- |
| Page / navigation background | `#FCFBF8` / `#F3F2ED` |
| Primary text | `#1B2430` |
| Secondary text | `#59636F` |
| Hover surface | `#E7ECDF` |
