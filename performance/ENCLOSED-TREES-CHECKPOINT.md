# Readable outlines inside quotations and callouts

2026-09-09. Layout-first continuation of Audit A07. The full audit remains active.

## Change

Labeled deep outlines rooted inside quotations and callouts can now use the
compact tree. Previously both contexts were excluded regardless of available
width. Fixture 94 reproduced the result: the deepest quoted explanation wrapped
into a narrow strip despite substantial space being occupied only by indentation.

The outline's pressure estimate now includes its outer enclosure, once at the
root: 48 px per quotation level (both sides) and 72 px for a callout's content
and trailing padding. Supporting containers deeper in an outline do not switch
individual branches independently. The existing 320 px pressure threshold,
64 px deep inset and canonical parent labels remain unchanged. At comfortable
widths the ordinary hierarchy remains in use.

Compact local connectors now include quoted list labels; supporting quotation
paragraphs without list markers retain their own treatment. Quote rails, callout
icon/title/surface, parent captions and branch returns remain visible. There is
no content collapse, source rewrite, additional document model or scroll-time
layout search. The UX skill informed preserving enclosure identity while
reclaiming indentation space; the Rust skill informed root-owned pressure and
geometry/source regression checks.

## Verification

- The native-font regression failed before implementation because quoted deep
  branches never entered compact layout.
- Passing geometry tests cover logical widths 360/520/570/620/1600 at
  100/150/200%, including different quote/callout transition widths. Deep prose
  retains at least 224 px on the 360 px canvas, every shaped line fits, enclosure
  bounds contain text and parent captions, and projected/source text is preserved.
- Retained typing/full rebuild checks now include both enclosed deep paragraphs
  at widths 360/1280 and all three zoom levels, including growth and shrinking.
- The explicit code-first/no-label fallback remains tested and unfinished.
- `scripts/check.sh` completed successfully: formatting, locked all-target checks,
  strict Clippy, workspace tests and doctests. Document-view: 459 passed, two
  ignored. `git diff --check` is clean.
- Crusty context `ctx_7a0fbcdd6a05`, validation `task_29a5a03b9d47b00e`:
  completed with 36 existing advisory findings, none new or worsened. Workspace
  checks ran separately.

## Native evidence

Runtime SHA-256:
`fc8637c5ae4626d1f725f030fc6df18687e4ba8000864d149a39ff409c22088c`.
Fixture `94-enclosed-trees.md` SHA-256:
`775d05d1fe545f00f9e1c08abf164bd4a797106d843aba335a3861eb3bd28c8a`.

Artifacts in `layout-previews/` retain runtime/source/input sidecars and use
private source copies. The following final-build previews were visually inspected:

| Prefix | Evidence |
| --- | --- |
| `enclosed-trees-narrow` | 400×1400 light, quote branch return and complete eight-level callout; appearance and exact unchanged source pass. |
| `enclosed-trees-alert-edit` | Native deepest-callout click/Home/type/autosave, preserved neighbors, focused idle and exact whole-file undo; typed screenshot inspected. |
| `enclosed-trees-quote-edit` | Equivalent native quoted-paragraph edit, preserved neighbors, focused idle and exact whole-file undo; typed screenshot inspected. |
| `enclosed-trees-wide` | 1600×1600 light, ordinary indentation and both complete enclosures; appearance and exact unchanged source pass. |
| `enclosed-trees-dark-200` | 1000×1600 dark, 200%, final callout branches and following heading; appearance and exact unchanged source pass. |
| `enclosed-trees-copy` | Twenty-four markers copied once each in source order, including parent labels reused by visual captions; appearance and exact unchanged source pass. Marker order/uniqueness is not full clipboard equality. |

`enclosed-trees-before` is the previous `bdcd8e54` runtime; `enclosed-trees-first`
is intermediate `5c7256be`, before root pressure and quoted local connectors.
Neither is final-build qualification. Edit-location checks permit canonical
escaping within an edited leaf; exact whole-file undo is checked separately.
Active AT-SPI supports these runs but is not a complete screen-reader audit.

## Remaining

Code-first/unlabeled items, outer definitions/metadata/table cells, richer mixed
enclosures and extreme nesting still need layout treatment and qualification.
The full RTL/IME/structural-edit/resize/accessibility matrix, other grammar
families, static/paged export and sustained release performance remain required.
This checkpoint does not sign off the complete hierarchy or callout family.
