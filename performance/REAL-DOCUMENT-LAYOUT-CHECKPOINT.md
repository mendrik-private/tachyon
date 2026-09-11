# Real-document layout review — 2026-09-09

Continues the full audit under the user's layout/available-space priority.
The unmodified repository README and plan were opened as native holdouts.
Their wording was not rewritten to make the layout qualify.

## Correction from the README

Short labels such as “Build and open a document:” occupied an almost empty
column beside their code block. The measured explanation candidate now rejects
an authored colon-ended lead-in shorter than three reference lines beside code.
It stays above its example, sharing the document leading edge. Longer measured
explanations still negotiate side-by-side placement; this preserves the README's
useful installation explanation/code pair. No filename or English keyword is
used by the planner. Source text and code whitespace are unchanged.

Native-app UX guidance informed the distinction between a label and a peer
explanation, and the shared leading edge. This is a readability/grouping
correction, not a claim of reduced document height. Stacking two brief labels
uses more vertical space but removes two mostly empty explanation columns.
Existing full-width code styling and readable prose measures remain unchanged.

The plan's introductory prose/list composition remains useful at both the
1600px baseline and 1280px final capture; no unrelated layout change was made.

## Safe holdout capture

`capture-layout.py --source-document PATH` copies exact Markdown bytes into a
private `holdout/` directory before launching the editable app. It never passes
the original path to the app. Relative images/resources are deliberately not
copied, so this mode alone cannot qualify a media-heavy source document.
`--fixture` rejects absolute and parent-traversal paths rather than allowing
them to escape the private session. Source-document and fixture options are
mutually exclusive; generated input cannot replace a requested holdout.

Tests prove byte-identical staging and that changing the staged copy leaves
the original untouched. Planning validation uses the staged initial file size,
not a nonexistent repository fixture or an approximate requested generator size.
The first `holdout-readme-before` and `holdout-plan-before` runs exposed that
old repository-path assumption after their screenshots/source reports were
saved. Those images remain visual baselines, not fully successful harness runs.
Subsequent native runs completed successfully with committed planning reports.

## Evidence

Final native layout-validation binary SHA-256:
`9ad0dde326dbed4af1dc57609ee102a45d23735534f47ca4c47166d16ba551cb`.
Source hashes, unchanged on the actual repository files after all checks:

- README: `abca9115a24f8b4293074b510011acfc5d11a036f910ba68d77e6f0054527541`
- Plan: `0e6d5a63b803ce3001c558e8747a46b2ffd376ee8a829a473141810deff47079`

Private Weston/D-Bus/AT-SPI captures in `layout-previews/`:

| Prefix | Scope |
| --- | --- |
| `holdout-readme-before` | 1600×1200, dark, old binary `89e5be63…`; short labels beside code |
| `holdout-readme-after` | 1600×1200, dark, final binary; short labels above code, substantial explanation still paired |
| `holdout-plan-baseline` | 1600×1200, light, old binary; successful baseline trace after harness correction |
| `holdout-plan-regular` | 1280×1000, light, final binary; introductory split retained and list text wraps |
| `holdout-readme-200` | 1920×1200, light, 200% text, scrolled to build instructions; readable stacked code examples |
| `holdout-readme-edit` | Pointer insertion inside the short label on a private copy; autosave, preserved code/next label, byte-exact undo |

All final rendering screenshots above were inspected and their source/appearance
checks pass. Captures inspect the visible opening/build sections, not every
section of either document. Native edit evidence is a targeted insertion plus
exact undo, not complete edited-source semantic equivalence.

The README-backed regression failed before the fix and passes afterward at
1040/1320/1640 logical canvas widths, including the positive longer-explanation
case. `scripts/check.sh` passes: formatting, locked workspace checks, Clippy,
all-target tests and doc tests; document-view has 422 passing tests and two
ignored. The capture-harness suite has 15 passing tests. `git diff --check`
passes. No new dependency, platform API, or document mutation path was added.

```sh
python3 performance/capture-layout.py --source-document README.md \
  --binary target/debug/tachyon --width 1600 --height 1200 \
  --appearance dark --source-unchanged-check --atspi-active \
  --layout-trace details --output performance/layout-previews/holdout-readme-after.png
```

Full real-document traversal, other grammar families, every width/scale/input
state, source/save audit dependencies and current-build release performance
remain open. The theme-policy question and current minimap exclusion are
unchanged. This checkpoint is not A07 or full-audit sign-off.
