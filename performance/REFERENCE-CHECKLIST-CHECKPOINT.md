# Compact reference and checklist composition

2026-09-09. Continues the user's layout/available-space priority and Audit A07.
The complete layout milestone and audit remain open.

## Layout change

Full-page review of fixtures 53 and 54 found an unnecessarily stacked pair in
53: a small properties table followed by a similarly sized task checklist.
The native-app UX skill's guidance on shared alignment and semantic grouping
informed the change. Compact H2 sections ending in an explicit task checklist
can now participate in the existing measured reference-section rows, alongside
table/code sections. Headings keep a common baseline and the actual component
sizes determine the split; the table is not stretched to fill spare space.

Eligibility is deliberately bounded: one simple checklist of at most six tasks,
optionally preceded by short introductory paragraphs. Ordinary H2 bullets,
mixed task/plain lists, rich or nested tasks, trailing explanations and sections
with subheadings do not gain this treatment. Existing H3 mixed list/prose peers
keep their previous classification. Source order, section boundaries, row
height limits and narrow-window fallback remain authoritative.

The regression test exposed two measurement discrepancies: reference candidates
omitted the 48 px checklist summary/header, and subtracted 16 px of trailing
space when the actual last task contained only 8 px. Candidate measurements now
include the header and clamp the trailing subtraction to realized spacing.
No paint-only offset, invented source structure or theme change was introduced.

At a 1314 px document canvas, fixture 53 selects a 533.5 / 756.5 px split with a
24 px gutter. Measured column heights are 263.5 / 290.5 px. The entire properties
and checklist pair is visible in the lower 1600×1200 native capture.

## Source-preservation defect found during verification

The new native checkbox oracle initially failed: clicking the first task
correctly updated its checkmark and progress summary, but autosave also changed
`grammar.` to `grammar\.`. A minimal core command/serialization regression,
`toggling_a_task_preserves_its_untouched_body_spelling`, reproduced this without
the UI or autosave. The command left the body unchanged; the item serializer
unnecessarily regenerated it when the checked state changed.

The bug-diagnosis skill's red/green loop guided this correction. When a changed
task state has unchanged, shared body blocks and a verified original marker,
source reuse now patches that marker byte alone. Existing serialization remains
the fallback for changed bodies or unsupported source structures. Tests cover
punctuation, emphasis, entities, links, code, ordered markers, tabs, CRLF,
nested/quoted multiline tasks, the complete fixture 53, reopening and exact undo.

## Verification

- `major_checklist_and_properties_share_a_measured_reference_row` verifies
  predicted/realized heights at logical widths 1040/1312/1632, stacked fallback
  at width 600 and short height 200, and unchanged source.
- Classifier tests cover both eligibility and explicit exclusions, including
  preservation of H3 mixed-peer classification.
- The existing retained-row typing/growth/undo differential test now includes
  this fixture's task body and table cell at 100/150/200% zoom. Local rebuilds
  match complete geometry and retain their bounded wrapping work.
- `performance/task_toggle_check.py` drives a real native checkbox click,
  requires exactly the intended source-marker change, then requires one native
  undo to restore every original byte. Its source oracle has three unit tests.

Final layout-validation binary:
`985eef9dd77da6fd612c5865f46593f621ad4fa3ee1e237900f426da2558c4d0`.

Native artifacts in `layout-previews/`:

| Prefix | Scope |
| --- | --- |
| `page-review-53-top`, `page-review-53-lower` | Original overview composition, binary e6fb3ab…; lower capture shows the stacked reference sections. |
| `page-review-54-top`, `page-review-54-lower` | Original complete mixed-peer/list/tree holdout review. |
| `reference-row-54-after` | Layout-change holdout, binary b8e3d13…; existing mixed trio remains intact. |
| `reference-row-53-toggle-qualified` | Final 1600×1200 light, 100%; before/toggled/undone frames, exact marker-only autosave and undo. |
| `reference-row-53-edit-qualified` | Final wide row; native task-body insertion, autosave, protected neighboring table/task fragments and exact undo. |
| `reference-row-53-narrow-qualified` | Final 600×1100 light; table and checklist stack in source order. |
| `reference-row-53-200-qualified` | Final 1600×1200 dark, 200%; enlarged table and checklist stack without clipping. |

Fixture 53 remains SHA-256
`fde6b28af6e959b6bd73d36be5e7cd34b0cb6b5bf26ae316a789017eb5f14b65`.
All captures use isolated private copies, not the user's working document.
The ordinary text-insertion oracle verifies the intended fragment and protected
neighbors, not byte-exact edited serialization; its undo is byte-exact.

Reproduce the marker check:

```sh
python3 performance/capture-layout.py --fixture 53-document-grammar.md \
  --binary target/debug/mineral-markdown --width 1600 --height 1200 \
  --appearance light --source-unchanged-check --atspi-active \
  --scroll 900 --scroll-steps 10 --scroll-settle-seconds 2 \
  --select 825 1008 825 1008 \
  --task-toggle-check 'Establish a shared visual grammar.' \
  --output performance/layout-previews/reference-row-53-toggle-qualified.png
```

Final screenshots, including toggled and text-edited states, were inspected.
`scripts/check.sh` passes formatting, locked workspace checks, strict Clippy,
all-target tests and doctests (433 document-view tests pass; two remain ignored).
All 111 Python harness tests pass, as does `git diff --check`.

Crusty preparations `ctx_9b5ba1bb30fc` (layout) and `ctx_89c29fec39fd`
(marker preservation) were validated: `task_cd9af0a2fca10c77` and
`task_4d67ac79a3d55c01` completed with 36 existing advisory findings and no new
or worsened findings. This is static advisory evidence, not runtime acceptance.

This checkpoint is not whole-grammar acceptance, the complete accessibility/IME
and resize-anchor matrix, export qualification, or release performance evidence.
Those remain open in the audit.
