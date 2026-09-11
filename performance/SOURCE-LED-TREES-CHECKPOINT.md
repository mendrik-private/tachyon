# Code, heading and figure-led branches

2026-09-09. Layout-first continuation of Audit A07. The full audit remains active.

## Layout

An outline item may now begin with a paragraph, heading, code block or image
without disabling the compact deep-tree layout. Parent context references that
actual node: code uses a bounded literal excerpt, a heading its authored text,
and an image its alt description. Empty source uses a role name, not an invented
authored title. Existing excerpt byte/grapheme bounds remain in force.

Code-first parent captions sit above the code header and preview, aligned with
the pane's outer edge. They do not occupy the language/Copy toolbar. Code-item
markers sit outside the pane, independently of its scrolling text/gutter.
Source depth, original markers and source order remain intact; comfortable-width
outlines retain their ordinary indentation. No source content is collapsed.

The UX skill informed separation of branch context from component controls. The
Rust skill informed one label-recognition path and shared caption/marker geometry.

## Save defect discovered during native verification

The initial code edit reached the intended code, but saving changed an untouched
child from `Code-led explanation` to `Code\-led explanation`. A reduced core test
reproduced the exact defect with one fenced-code parent and one child item:

```text
cargo test -p document-core --test source_fidelity editing_code_parent_keeps_unmodified_child_source --locked
```

The source reuse path had no fenced-code leaf span, so it regenerated the entire
parent item, including its unmodified descendants. The diagnosis skill supplied
the minimal red/green loop; the pointer-location probe ruled out missed clicks.

Import now records original-file fenced-code spans. Saving replaces that leaf
using the canonical, fence-safe code serializer and its original continuation
context. Unmodified descendants and separators are copied from source. Literal
CRLF and inserted LF are retained without double-inserting carriage returns.
Synthetic HTML imports cannot contribute original-file spans; unsupported or
structural edits still use semantic serialization rather than hiding changes.

## Verification

- Source-led layout regression failed before eligibility was extended.
- Native-font tests cover widths 360/620/1600 at 100/150/200%, source identities,
  parent excerpts, projected content (excluding line delimiters), real prose and
  heading fit, caption/code-header separation, and markers outside code panes.
- Retained typing/full geometry tests now include the code parent, heading and
  both child explanations at widths 360/1280 and all three zoom levels.
- Two new source-fidelity tests cover the minimal failure and 48 combinations of
  marker/quote/callout/depth/final-newline context, LF/CRLF and inserted text,
  including Unicode and fence-like backticks. They check untouched child spelling,
  semantic reopening, undo/redo, plus whole-file equality for fixture 95's edit.
- `scripts/check.sh` completes successfully: format, locked all-target checks,
  strict Clippy, workspace tests and doctests. Document-view: 460 passed, two
  ignored; source-fidelity integration suite: 20 passed. `git diff --check` clean.
- Layout context `ctx_2acc404fa8e1`, validation `task_d872a6657d314a90`; save
  context `ctx_9be943ab4740`, final validation `task_f11cdf42770b4cee`: both
  completed with 36 existing advisory findings, none new or worsened.

## Native evidence

Final runtime SHA-256:
`ee418802e2e8a55b29976bbde8d7656371ce0ca7447aa9908f078f435033acf7`.
Fixture `95-code-first-tree.md` SHA-256:
`37aea13f5ce58462f331f0b7a9d2db8820761cdf181aec23a7c0c932ed93d738`.

Final artifacts in `layout-previews/` use private source copies and retain input,
runtime and source sidecars.

| Prefix | Evidence |
| --- | --- |
| `source-led-final-edit` | 400×1400 light, native eighth-level code click/Home/type/autosave, complete expected edited-file equality, protected neighbors, focused idle and exact whole-file undo. Typed pixels show the child caption updating to the edited code without moving the branch. |
| `source-led-final-wide` | 1600×1800 light, complete ordinary outline and intrinsic-sized figure; source/appearance pass. PNG byte-identical to the inspected `source-led-wide` before the save-only fix. |
| `source-led-final-dark-200` | 1000×1600 dark at 200%, readable ninth-level code/image child explanations and branch returns; inspected, source/appearance pass. |
| `source-led-final-copy` | Sixteen unique markers copied once each in canonical order, including code and alt text reused by captions; source/appearance pass. Marker order is not full clipboard equality. |

`source-led-before` uses previous runtime `fc8637c5`. `source-led-first` predates
the marker correction; `source-led-narrow`, `source-led-wide`,
`source-led-dark-200` use `f02e60f4` before the save fix. The
`source-led-code-edit` run failed the untouched-neighbor oracle; the reduced
`source-led-edit-probe` passed insertion/undo without checking those neighbors
and is diagnostic only. `source-led-code-verified` uses intermediate `17056d99`
before the CRLF correction. None replaces the final qualification above.

## Remaining

Container-first and non-textual/empty-parent hierarchy cases still need explicit
layout treatment. Heading/image/paragraph-parent edits need equivalent complete
descendant-byte qualification; this code-leaf fix does not prove those paths.
Indented code, arbitrary footnote/definition contexts, changed enclosing syntax,
tabs, full structural editing, RTL/IME/resize/accessibility, other grammar families,
static/paged export and sustained release performance remain open. This does not
sign off the complete hierarchy or editing family.
