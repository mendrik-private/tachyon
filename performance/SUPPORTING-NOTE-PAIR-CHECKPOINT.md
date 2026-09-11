# Supporting callout pairs — 2026-09-09

Continues the layout/available-space priority and A07. A source-adjacent pair
of short Note/Tip callouts can now occupy one supporting column beside its
prose. Previously the first note moved beside the explanation while the second
was stranded below it. This is a bounded extension of the existing Aside
composition, not general anchor-linked margin notes or paged layout.

The native-app UX guidance informed the shared alignment, natural panel heights
and source-order fallback. The section heading stays full-width. Existing
loaded-font width, overflow, relative-height and viewport-fit gates still apply;
type is not reduced to force a pair. The shared 24px gutter and measured gaps
between callouts remain intact. Narrow or short windows stack the whole group.

Recognition consumes at most three existing search units: main prose and two
notes. Larger clusters, a critical/ordinary quote in the cluster, unsupported
children or excessive note text leave the entire cluster in normal flow.
Canonical roots beyond a bounded search-window edge are checked so that the
window cannot silently truncate a note cluster. Existing editing locks retain
both callouts' source-owned slots while text grows.

## Verification

- The new supporting-column test failed before implementation: the note part
  contained one root instead of two. It now passes, comparing candidate heights
  with final visual-line footprints and checking narrow/short fallback/recovery.
- Negative tests cover second-callout warnings, ordinary quotes, code, long
  text and three-callout clusters. A separate test truncates the search window
  before/inside the pair and requires no partial nomination.
- The existing editor-localized versus full-layout regression now edits both
  callouts at 100/150/200% zoom, including growth and exact undo restoration.
- `scripts/check.sh` passes: locked dependency checks, formatting, workspace
  check, Clippy, all-target tests and doc tests. Document-view: 421 passed,
  two ignored. `git diff --check` passes.

Fixture `81-supporting-note-pair.md` SHA-256:
`e248c6085e83941457c4152a321515770e7a05c1d1d8c6a6fa97f5cc4eb94fdf`.
Native layout-validation binary SHA-256:
`89e5be63c215e7c994d605f5cb116b6274061147b4a7b4cde0256d879c435070`.
Only test additions followed this binary build, not production changes.

Native captures use isolated Weston/D-Bus/AT-SPI and disposable source copies:

| Prefix in `layout-previews/` | Evidence |
| --- | --- |
| `note-pair-before` | Old binary `5b395270…`; first note beside prose, second below |
| `note-pair-after` | 1440×1100, dark, 100% text: both callouts in the supporting column |
| `note-pair-narrow` | 600×1100, dark, 100% text: complete source-order stack |
| `note-pair-200` / `note-pair-200-scrolled` | 1920×1100, light, 200% text: stacked notes, including a scrolled view of both panels |
| `note-pair-hidpi` | 1920×1200, dark, 100% text, requested compositor scale 150/120: paired composition retained |
| `note-pair-edit` | Native pointer edit within the second callout, autosave and byte-exact undo; first note/main prose markers preserved |

All listed rendering screenshots were inspected; source-unchanged and
appearance reports pass. The edit capture's six-marker copy-order check passes,
but is not a full clipboard-equality assertion. The fractional-scale request
alone is not a complete physical-pixel scale qualification.

In the ordinary 1440px specimen the following heading moves from y=662 to y=642,
20 logical pixels less vertical travel with all source content intact. The
larger improvement is keeping the two supporting panels together with equal
widths and a shared leading edge. This does not claim maximal fill of an
ultrawide screen: the existing readable main-text measure remains bounded.

Native continuous resize, full keyboard/IME/RTL and structural-editing cases,
general anchored notes, other grammar families, source/save audit dependencies
and release performance remain open. Theme policy is unchanged pending the
previously requested user choice; the current plan still excludes the minimap.
No full A07 or audit sign-off is implied.
