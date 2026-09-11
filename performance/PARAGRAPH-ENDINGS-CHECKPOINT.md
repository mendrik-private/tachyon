# Paragraph endings

2026-09-09. Layout-first continuation of Audit A07. This is bounded typography
polish, not completion of the layout grammar or broader audit.

## Change

An isolated short word on a paragraph's final line can now gain two to four
preceding words. Only the last two automatically wrapped lines change; their
combined source range, line count, font size and available width stay intact.
The same native styled-font measurements feed planning and editor geometry.
Among at most three legal candidates, the closer pair of line widths wins.

The refinement only applies to ordinary paragraph endings whose final word
uses at most a quarter of the available width. It preserves grapheme boundaries
and introduces breaks only at ordinary spaces. Headings, tables, metadata,
captions, bibliography, definitions, resource titles, hard breaks and specialized
inline content retain their existing behavior. Markdown soft breaks already
project as ordinary spaces. The existing RTL wrapping path is unchanged.

Work is bounded to paragraphs at most 16 KiB and a final two-line tail at most
2 KiB. At most seven additional intrinsic-width requests are made on a cold
eligible wrap; the finished ranges are cached. Eligibility participates in cache
identity, so a refined paragraph cannot contaminate an excluded context.
No source rewrite, alternate document state or scroll-time layout search is added.

The UX skill informed the restrained final-line treatment and stable vertical
rhythm. The Rust skill informed shared measurement ownership, explicit work
bounds and cache/source regression coverage.

## Verification

- A native-font regression failed before the refinement. It now finds a native
  single-word witness at 100/150/200%, verifies at least three final-line words,
  equal line counts, unchanged earlier lines, fitting measured widths and exact
  source coverage. Warm paragraph and excluded-context wraps require no shaping
  or intrinsic-width requests. Cold requests remain within the stated bound.
- Eligibility and boundary tests cover hard breaks, code, links, math, images,
  tables, headings, oversized paragraphs/tails, combining marks, NBSP, tabs and
  CJK fallback. Bold/italic/strike remain eligible and retain styled measurement.
- The existing retained-typing/full-geometry regression passes, including both
  deep explanatory paragraphs at 100/150/200% with local shaping bounds and undo.
- `scripts/check.sh` passes formatting, locked all-target checks, strict Clippy,
  workspace tests and doctests: 452 document-view tests pass, two ignored.
  `git diff --check` is clean.

## Native evidence

Fixture: `layout-fixtures/91-nested-reading-measures.md`.
Unchanged source SHA-256:
`d21458c7670a48c3e22079dd4906175f96ffa25d620daa6c00ac39404339afda`.
Baseline runtime:
`887e56cb1d1bf78e43c790c43284695371095e2d19bfaf7a5033e1b9c0870681`.
Final runtime:
`e73c8e843976381bc3be3dc0c4aef9cdee6d6863880076e1cdf3633fde1d4f1d`.
Only tests, comments and documentation changed after that runtime build.

All captures are in `layout-previews/`, use private source copies and were
visually inspected. Sidecars retain the actual binary and source evidence.

| Prefix | Evidence |
| --- | --- |
| `nested-measures-after` → `paragraph-endings-after` | Same 1600×1200 light fixture. “them.” becomes “the notes that explain them.”; “document.” becomes “searching elsewhere in the document.” The following section retains its position. |
| `paragraph-endings-narrow` | 600×1100 light, scrolled. Deep paragraphs retain readable wrapping and stay inside the window. |
| `paragraph-endings-200` | 1600×1200 dark, 200%, scrolled. Nested text, markers and guides remain legible and source-complete. |
| `paragraph-endings-selection` | Native Home/arrow/Shift selection across the adjusted wrap copies exactly “without searching”, with byte-identical source. The selected-state screenshot shows both lines. |
| `paragraph-endings-edit` | Native click/Home/type at the adjusted final line, intended source-span insertion, preserved neighboring fragments, autosave, stable focused idle and exact whole-file undo. Typed and idle screenshots retain the edited state. |

The typing location oracle permits canonical punctuation escaping inside the
edited leaf; it is not an assertion that the entire edited file differs by only
one byte. Undo is a whole-file equality check. Active AT-SPI is supporting
evidence, not complete screen-reader qualification.

## Remaining work

This does not implement global paragraph balancing, hyphenation, page/column
widow-orphan control, or arbitrary multi-script typography. Protected inline
paragraphs conservatively retain native wrapping. Deep narrow-window responsive
tree presentation remains the next larger hierarchy task. The full interaction,
IME/RTL/accessibility and structural-edit matrix, remaining grammar families,
static/paged export and sustained release performance gates remain open.

Crusty context `ctx_1079ca944e55`, validation `task_738c824c2668637b`:
36 existing advisory findings, none new or worsened. Local workspace checks
ran separately. A07 remains active; no full-audit or family completion is claimed.
