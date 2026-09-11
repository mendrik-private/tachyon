# Table reading edges

### Typography and palette

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

<!-- tachyon-table:v1 {"border":"Dotted","widths":[380.078125,230.203125]} -->
| Token | Value |
| --- | --- |
| Page \/ navigation background | `#FCFBF8` \/ `#F3F2ED` |
| Primary text | `#1B2430` |
| Secondary text | `#59636F` |
| Hover surface | `#E7ECDF` |
| Floating surfaces | `#FFFFFF` |
| Links and active controls | `#256F50` |
| Selection | `#DCEBE1` |
| Table rules | `#DEDFD7` |
| Errors | `#9E4B3F` |

Use a 4 px spacing scale. Paragraph spacing is 16 px; ordinary headings have 20 px above and 7 px below. H1/H2 use a subtle gray text shadow, and chapter numbers have distinct green sans typography. Quotes have inset padding; cards have quiet backgrounds and fine outlines. Avoid textures, gradients, and page shadows. Floating controls use an 8 px corner radius and a restrained shadow. Keyboard focus remains visibly outlined.

