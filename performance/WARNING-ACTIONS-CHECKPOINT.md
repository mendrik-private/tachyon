# Keep critical conditions with their actions

2026-09-09. Layout-first Audit A07 continuation, C10. The full audit remains active.

## Composition

A short top-level Warning, Caution or Important followed immediately by a fenced
example or ordered procedure now forms one source-order composition unit. A short
colon-ended lead-in may sit between them. The warning stays above the action;
outer row search cannot extract either into an unrelated column. Internal edges
use the shared 12px `INSTRUCTION_GAP`; headings retain normal section spacing.
Existing labels, icons, numbered steps, code controls and responsive alert layouts
are unchanged. Nothing moves across a heading or rewrites Markdown.

Recognition is bounded to one/two paragraph alerts, each at most 480 bytes, and
an optional lead-in of at most 240 bytes. Note/Tip, unordered facts, intervening
ordinary prose, headings and longer alerts retain ordinary flow. These bounds
limit recognition; they do not truncate the document or shrink its text. The
relationship is source adjacency with explicit critical severity and technical/
ordered structure, not a fabricated warning or an English command-word guess.

This is continuous-document attachment, not a claim that warning/action page
break handling or every nested/structured-alert variant is implemented.

## Shared-gallery ownership correction

Review found that the preceding shared-caption implementation added an explicit
edge from the final gallery root into the separate caption group, while
`GroupAnalysis::validate` required every edge endpoint to belong to the same
group. This made the semantic-plan validity gate reject those documents, even
though later row measurement could still render galleries. The earlier native
screenshots were insufficient to reveal the skipped plan initialization.

A new regression fails on that state. Validation now permits only the explicit
GalleryCaption edge from the immediately preceding canonical root into the first
caption root. It still rejects nonadjacent edges and preserves exact, unique
root/node ownership. The current gallery preview restores full plan initialization
(including lead typography), and native End/Shift-Home copying is rechecked.

## Verification

- The initial critical-warning grouping test fails because warning and action
  have different group identities. It passes for three severities, code/ordered
  actions and with/without a lead-in. The canonical partition validates and the
  source is unchanged.
- Negative tests retain boundaries, ordinary prose, Note/Tip, unordered lists
  and long warnings. The gallery ownership test rejects a deliberately changed,
  nonadjacent attachment after accepting the real shared-caption relationship.
- Native-font geometry checks logical widths 360/760/1280 at 100/150/200% font
  environments. Four explicit fixture edges have 12px gaps, no overlap and no
  horizontal displacement separating warning from action. Source remains exact.
- `scripts/check.sh` exits 0: formatting, pinned/locked metadata and all-target
  checks, strict Clippy, 474 view tests (two ignored), 116 core, 25 source-fidelity,
  11 tree-selection, one external-link and 39 app tests, plus doctests.
  Log: `/tmp/mineral-warning-actions-qualified-check.log`. `git diff --check` passes.
- Crusty contexts `ctx_83f1f740661c` and `ctx_a6ebe08d18d2`, implementation
  validation `task_aff0c1d92ae4eb6d`: completed, 36 existing advisory findings,
  none new or worsened. No new dependencies or source-serialization changes.

The UX skill informed attachment gaps and intact warning/action hierarchy;
the Rust skill informed source partition validation and independent geometry
oracles. A first test draft referenced private planner internals; the measured
test now derives its expected edges from fixture content instead of widening APIs.

## Current native evidence

Runtime SHA-256:
`70e174df1e07935551cc6c52b8865492d2d05900331b37b017325b6d19a6b565`.
Fixture `99-warning-actions.md` SHA-256:
`f368c5ce8f9f20319e04b95c3a5178151ff913308b0111bba81c50f471d306b9`.
Gallery fixture 98 remains `c24f6219c99e896fc08330793a2779e93630ee709b292a51c95c6b4bcfe2590b`.
All artifacts are in `layout-previews/`, with private-session source/runtime/
input/appearance sidecars. No user document was edited.

| Prefix | Evidence |
| --- | --- |
| `warning-actions-wide` | 1000×1500 light. Inspected all three critical severities, lead-in/command strip, ordered procedure, configuration pane and unjoined heading boundary. |
| `warning-actions-narrow` | 400×1800 light. Inspected stacked alert header/body, compact attachment and intact procedure order. |
| `warning-actions-dark-200` | 1000×1600 dark at 200%, 12 wheel steps. Inspected enlarged caution/ordered procedure and important/configuration attachment. |
| `warning-actions-edit` | Native click/Home/type in warning body, autosave, idle and undo. Complete edited-file equality and exact original-file undo; typed pixels retain the warning/action group. The edited paragraph uses the existing period escaping. |
| `shared-gallery-valid-ownership` | 1600×1700 light. Inspected initialized lead typography, intact gallery, shared caption and credit. Source/appearance pass. |
| `warning-actions-code-edit` | Native click/Home/type in the attached command, autosave/idle/undo. Full edited-file equality and exact original-file restoration pass. |
| `warning-actions-copy` | Ten unique markers occur once in canonical warning/action order. This checks marker order, not entire clipboard equality. |
| `shared-gallery-valid-end` | Native End/Shift-Home copies the complete shared caption exactly on the corrected valid-plan runtime. Source unchanged. |

`warning-actions-before` is the previous runtime, not final qualification.

## Remaining

Nested warnings, rich alert bodies, chains of critical warnings, arbitrary action
encodings, long-growth/structural/RTL/IME/missing-content state matrices, paged
keep-with-next and static exports remain open. Full grammar coverage and sustained
release-performance qualification retain their original requirements.
