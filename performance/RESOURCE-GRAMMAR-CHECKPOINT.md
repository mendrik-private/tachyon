# Resource objects — verified checkpoint

Scope: Board 03 resource cards and compact linked rows, not approval of the
complete grammar. Written tokens in `designs/document-design-grammar.md`
remain authoritative. This checkpoint uses release binary SHA-256
`24284f0299b97b5a18c956a672efefc714c6b6da47fb8c4d781a1d8dc9535a2d`.

## Implementation

An authored leading link with an explicit title/description separator is a
resource object. Link-only objects use compact rows. Ordinary linked prose,
tasks, ordered instructions and paragraphs with competing destinations are not
converted. Source-owned titles, delimiters and descriptions remain editable;
there are no fetched previews, invented captions or layout controls.

Candidate fit and rendering share one font-aware geometry constructor. Cards
use 24 px insets, a 16 px link icon plus 8 px gap, an 8 px title/body gap,
natural heights, Paper, a 1 px Rule boundary and 4 px corners. Compact rows use
12 px insets and one bottom rule. Columns retain row-major reading order and
stack when actual fit or height balance fails. Focused edits retain placement.

## Native evidence

Fixture: `layout-fixtures/58-resource-objects.md`, source SHA-256
`5d4f19fb7f601a45c496163af4d6db6017a8e8808c8e541d88dd2c3fc1d7146f`.

- [1600 × 1200](layout-previews/resource-verified-wide.png): three cards,
  compact rows, readable standalone object and uneven vertical fallback.
- [600 × 1100](layout-previews/resource-verified-narrow.png): stacked objects.
- [600 × 1100 at 200%](layout-previews/resource-verified-200.png): wrapped
  titles and descriptions without overlap. Corresponding `.source.json`
  checks prove source unchanged.
- [Native edit](layout-previews/resource-verified-wide.edit.json): typing
  autosaved and undo restored exact bytes.
- [Copy order](layout-previews/resource-verified-wide.copy.json): six markers
  occur once in source order; this is not an exact full-document golden.
- [Warm geometry](layout-previews/resource-verified-wide.planning.json):
  explicit warm passes reuse published geometry with zero shaping/wrapping,
  zero segments laid out and zero anchor displacement.
- [Native link navigation](layout-previews/resource-verified-link.navigation.json):
  Ctrl+click opened fixture 57, typing hit its exact heading, undo restored
  bytes, and navigation left both sources unchanged.
- [Weston AT-SPI](layout-previews/resource-verified.atspi.json): resources
  advertise a Click action and activation opened fixture 56. These older
  pre-refresh accessibility bounds are not grid geometry evidence. The
  zero-refresh MCP compositor needed further input before publication.

A separate fixture-56 navigation probe reached the correct document but failed
its exact post-typing source oracle. Its cause is not established; the passing
fixture-57 probe does not prove every destination's exact serialization.

At 1× in the wide screenshot, Paper `(250,249,246)` surrounds Rule
`(218,221,213)`: top samples `(320,329..331)` are Paper/Rule/Paper;
left samples `(276,360)` and `(277,360)` are Rule/Paper. The first right
edge is x=686 and the next left edge x=711, matching the 24 px logical gutter.
Font-aware unit tests independently check 24/8 insets/gaps, 64 px section
separation, natural heights, zoom and retained UTF-8 source boundaries.

## Checks and performance

`cargo test --workspace --all-targets --locked`: 466 passed, 2 ignored.
`cargo fmt --all -- --check` and workspace Clippy passed. Crusty validation
reported no new or worsened architecture findings; compiler checks were run
directly because the earlier Crusty runner's Clippy invocation was invalid.

[60-second 10 MiB release run](layout-previews/resource-verified-perf.json):
107.87 average presented fps, presentation p99 11.70 ms, draw p99 6.29 ms.
There were four presentation intervals ≥25 ms (maximum 34.70 ms), one
application stall ≥25 ms, and 37 missed 120 Hz deadlines / 6511 opportunities.
This exceeds 60 fps on average, not on every frame. The generated mixed
workload is not exhaustive coverage of resource-heavy or future grammar.

Authored thumbnails, complex/nested objects, exhaustive interaction/RTL
coverage and the complete cards/signals board remain incomplete. The full
grammar goal stays active.
