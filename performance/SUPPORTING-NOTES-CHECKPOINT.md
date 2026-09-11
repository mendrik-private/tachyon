# Supporting notes beside prose — 2026-09-09

Implements a mixed-document layout improvement under STR-0001/STR-0003 and A07.
The full audit remains active. This is a source-adjacent paired module, not a
general margin-note anchor, float, or pagination implementation.

## Behavior

A short authored Note or Tip immediately following prose in the same section
can occupy a supporting column. The section heading remains full-width. The
main explanation uses the wider track; the note retains its real icon, label,
semantic color, natural height, and editable body.

The UX guidance informed two choices: preserve shared leading edges and the
24px gutter, and cap the main track at the loaded-font reference measure.
An ultrawide window therefore does not stretch the paragraph into an unreadably
long line merely to fill the screen. Candidate width still uses the existing
twelve-track system and deterministic row search.

Only adjacent Note/Tip paragraphs qualify. Warning, Caution, Important,
ordinary quotations, nested technical content, unrelated sections, long notes
and very short introductions remain stacked. Candidates require at least four
measured main-text lines, adequate width, bounded relative height, no overflow
and viewport fit. Narrow/short windows and larger text can stack; restoring
space permits the paired arrangement again. Editing retains the existing row
lock and source-owned identities.

The row measurer now handles paragraph-only alert containers using the same
header/inset/wrapping behavior as final geometry. Unsupported containers still
decline candidate measurement. Initial tests exposed missing alert measurement;
the resulting unmeasured candidates stayed stacked until this was implemented.
The final tests compare measured heights against rendered row footprints.

## Evidence

Fixture: `layout-fixtures/80-supporting-notes.md`, SHA-256
`a4e6622470cc5e15fb1ee6a289be367abd967338e2bd0f0f649c8c5eee8a6946`.
Final debug `layout-validation` binary SHA-256:
`ae97523abfde006fe580e09a82e96b32ce25d75910cdcb400ab81c37517eb670`.
Pre-change `supporting-notes-before` uses the earlier `a10f3372…` binary and
remains baseline evidence, not a final-build pass.

In the 1440 × 1100 dark capture, the document canvas is 1160px wide. The main
track is about 548px, followed by a 24px gutter and a roughly 384px supporting
track. The reference prose measure and natural callout chrome remain bounded.
The second heading moves from y=662 to y=554, and the third from y=1061 to y=869:
**192px less vertical travel across the first two sections**, without deleting
or summarizing any text. These are fixture-specific native AT-SPI bounds, not a
universal density or performance claim.

Final private-Wayland captures (all 100% display scale):

| Prefix in `layout-previews/` | Window / text zoom | Result |
| --- | --- | --- |
| `supporting-notes-after` | 1440 × 1100 / 100%, dark | Two optional notes beside their prose; warning stays in sequence |
| `supporting-notes-wide-light` | 1920 × 1100 / 100%, light | Readable bounded paired layout; system light palette |
| `supporting-notes-narrow` | 600 × 1100 / 100%, dark | Source-order stack |
| `supporting-notes-zoomed` | 1920 × 1100 / 150%, dark | Paired at the remaining usable width |
| `supporting-notes-200` | 1920 × 1100 / 200%, dark | Stack; no content removal |

Dark wide, narrow, 150% and 200% screenshots were inspected. Each listed view
capture has passing source-preservation, active AT-SPI and appearance reports.
`supporting-notes-edit.edit.json` verifies a native pointer insertion inside the
Note body, autosave, preservation of the main-prose marker and byte-exact undo.
Its copy report verifies canonical order of six markers through the paired
sections and following warning section, not full clipboard equality.

`scripts/check.sh` passes: locked dependency checks, formatting, workspace check,
Clippy with denied warnings, all-target tests and doc tests. View tests:
417 passed, two intentionally ignored native-font integrations. New tests cover
actual-font pair geometry and height agreement, narrow/short fallback and
recovery, semantic exclusions, and readable main-track width. The existing
localized-edit/full-geometry regression now also edits prose and Note/Tip bodies
at 100%, 150% and 200%, including growth and undo. `git diff --check` passes.

```sh
python3 performance/capture-layout.py --fixture 80-supporting-notes.md \
  --binary target/debug/tachyon --width 1440 --height 1100 \
  --appearance dark --zoom-steps 0 --source-unchanged-check --atspi-active \
  --layout-trace details --output performance/layout-previews/supporting-notes-after.png
```

## Remaining qualification

Multiple anchored margin notes, arbitrary mixed-content rails, native continuous
resize/scroll of the new pair, fractional display scales, minimap agreement,
IME/RTL/accessibility actions and release performance still need qualification.
The source/save audit dependencies remain open. This checkpoint does not sign
off the full design grammar or A07.
