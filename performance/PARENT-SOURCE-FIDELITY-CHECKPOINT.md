# Editing source-led parents without rewriting descendants

2026-09-09. Layout-first continuation of Audit A07. The full audit remains active.

## Scope and cause

This qualifies the paragraph-, heading- and image-led branches added in
SOURCE-LED-TREES-CHECKPOINT.md. It changes saving, not layout geometry.

A minimal parent edit reproduced the previous code-parent failure for ordinary
text: editing `a` in `- a` also changed an untouched child from
`Child-led explanation` to `Child\-led explanation`. The child was not dirty;
the serializer lacked its parent's leaf span and regenerated the whole item.
The diagnosis skill supplied the exact-file red/green loop. The Rust skill
informed source-boundary handling and layered regression verification.

Original-file leaf spans now cover paragraphs, headings and standalone images
as well as fenced code. The canonical serializer owns escaping inside the edited
leaf; descendants, separators and unrelated source syntax remain original.
Continuation prefixes preserve list indentation and quote rails. Task markers
are not extra continuation columns. Synthetic HTML spans remain excluded.

Definition paragraphs retain their dedicated path: Comrak's definition-term
coordinates and empty/split-term semantics cannot safely use this generic leaf
replacement. The first broad-span experiment failed two existing definition
tests; excluding definition text from the new path restored both without changing
their assertions. Unsupported structural changes still use semantic fallback.

A second red test covered toggling a parent task and typing in it, in both orders.
The previous checkbox-only shortcut could not compose with a body edit. Checkbox
and recursive body patches now share a rollback boundary: either both are safe
source-local edits, or both are discarded before canonical fallback.

## Verification

- Five added source-fidelity tests bring that suite to 25 passing tests.
- The parent matrix covers four forms (paragraph, heading, image, linked image),
  three marker styles, ordinary/quoted/callout contexts and LF/CRLF: 72 cases.
  Each asserts whole edited-file equality, an unchanged child dirty state,
  semantic reopening, exact undo and redo. Additional task/nested cases pass.
- Escaping and Unicode insertions, hard-break task continuations, both orders of
  task toggle plus typing, and the real `SetImageAttributes` command preserve
  descendants. Image changes retain the enclosing link and its title.
- `scripts/check.sh` completed with exit 0: format, pinned locked metadata,
  all-target checks, strict Clippy, workspace tests and doctests. Counts: 116 core,
  25 source-fidelity, 11 tree-selection, 460 view (two ignored), one external-link
  and 39 app tests. `git diff --check` passes.
- Crusty context `ctx_4f79190f3dd4`, preparation `task_73d2937aa89c0f5f`,
  validation `task_5f304d791c62fbc0`: completed, 36 existing advisory findings,
  none new or worsened; no blocking quality constraint matched.

## Native evidence

Final runtime SHA-256:
`85d421122450bf6b0ea66289dc737b058d1a23c56f39330e870e1bf93780de8d`.
Fixture `95-code-first-tree.md` SHA-256:
`37aea13f5ce58462f331f0b7a9d2db8820761cdf181aec23a7c0c932ed93d738`.
Artifacts in `layout-previews/` use isolated native sessions and private source
copies, with runtime/source/input sidecars.

| Prefix | Evidence |
| --- | --- |
| `parent-final-heading` | 400×1400 light, click/Home/type in `Evidence heading`, autosave, exact complete edited file, focused idle and exact complete undo. Inspected typed image shows the parent caption updating without shifting the branch. |
| `parent-final-paragraph` | Same native sequence in `Review notes`, with full edited-file and undo equality. Inspected typed image shows the child caption updating; other content stays unchanged. |
| `parent-final-wide` | 1600×1800 light, appearance/source pass. PNG byte-identical to `source-led-final-wide`: the source-only changes preserve the preceding wide-layout geometry and pixels. |
| `parent-final-dark-200` | 1000×1600 dark at 200%, appearance/source pass. Inspected: deep code/image explanations retain readable width, bounded context captions and visible branch returns. |

The earlier `parent-heading-edit` capture uses intermediate runtime `b7883504`;
it is diagnostic evidence, not a substitute for final-runtime qualification.
The initial minimal parent and compound-task tests failed as described above;
their final equivalents pass without weakening the byte or semantic oracles.

## Remaining layout-first work

Container-first and non-textual/empty-parent hierarchy cases remain open, as do
arbitrary definition/footnote contexts, indented code, tabs, enclosing-syntax
changes and the full structural-edit matrix. The image attribute command is
tested at the core seam; this checkpoint does not qualify every native image
editor interaction. RTL/IME, resize/accessibility, other grammar families,
static/paged export and sustained release performance still need qualification.
This checkpoint does not sign off the complete hierarchy or editing family.
