# Table structural editing and history — September 10

Resumed codex-work session `01a0725f-6ed9-7e22-8a1c-e2472d500d0f`
from its interrupted table-row regression. The inherited objective remains
specification completion followed by audit implementation, with the user's
later layout-first priority. This checkpoint closes only the bounded row
editing defects below; A07, T03, and the full audit remain unfinished.

## Corrections

The recovered `table_row_menu_changes_keep_the_surviving_caret_visible`
regression failed on current source: row insertion/deletion transformed the
selection correctly but could leave its caret outside the painted viewport.
`apply_structural_command` now reveals the surviving selection using the
geometry produced by `refresh_after_transaction`.

The real popup-menu workflow then exposed a separate Undo defect at 200%.
After inserting a row and undoing, source and selection were correct but the
caret occupied y=20..62 while the viewport started at y=54. Both Undo and Redo
now reveal the restored selection after refreshing the projection. No second
geometry system or selection transformation was introduced.

The regression now exercises insertion/deletion at 100% and 200%, exact source
and selection restoration, Redo, horizontal/vertical visibility, and history
after deliberately scrolling away. It failed before the history correction
and passes afterward. The recovered Tab-to-append regression also passes,
including the existing empty final cell and exact source Undo.

## Native evidence

`table_history_check.py`, invoked through
`capture-layout.py --recorded-bugs-check table-history`, drives Find, actual
pointer context menus, row insertion/deletion, and keyboard Undo/Redo in the
existing private Wayland seat and accessibility bus. It checks all cell names
and row counts against the original table, unchanged bytes outside the table,
exact source restoration on Undo and Redo, restored selection offsets, editor
focus, and full native caret visibility. Structural serialization inside the
changed table is checked semantically, not claimed byte-preserving.

Final binary SHA-256:
`e57e8fc2a5297313304bda299ff1b09a797cad8e1c5ab089a56358e113e6ed81`.
Fixture122 SHA-256:
`c3ae9753a2fdec364181e33a95d8364cf8311ec09deeb22ea3c86de8a13d0e35`.

Both final 1440x500 native runs pass:

- `layout-previews/table-history-final-100.bugs.json`
- `layout-previews/table-history-final-200.bugs.json`

Each report covers Insert row above and Delete row, with Undo/Redo/Undo and
the complete original file restored afterward. Menu and changed-state
screenshots accompany the reports; the enlarged deletion screenshot was
visually inspected.

The pre-fix binary `22847f21...` passes this native case at 100% but fails the
identical 200% Undo check. See `/tmp/mineral-table-history-before-200.log` and
the `table-history-before-200-*` captures. This is specifically an enlarged
text negative control; no native failure is claimed for its 100% run.

Reproduce a final case (add `--zoom-steps 10` for 200%):

```sh
cargo build --locked -p markdown-app --features layout-validation
python3 performance/capture-layout.py --fixture 122-paired-records.md \
  --binary target/debug/mineral-markdown --width 1440 --height 500 \
  --recorded-bugs-check table-history \
  --output /tmp/table-history.png
```

`scripts/check.sh` passes with 773 Rust tests, two existing ignored tests,
formatting, locked checks, strict Clippy, adapter tests and doctests. Log:
`/tmp/mineral-session-resume-final-check.log`. The capture harness's 20 Python
tests and recorded-table oracle's four tests pass; `git diff --check` is clean.
Crusty context `ctx_34d71c405bd2`, validation `task_60da3a964bf102fd` reports
75 existing advisory findings and no new, worsened, or resolved findings.

## Remaining work

The native screenshots exposed a right-edge Table submenu extending beyond
the output. Crusty `PRB-0160` records it. The harness distinguishes the Table
operations submenu from the identically named Insert block action and clicks
the submenu's exposed hit area. This does not qualify popup placement.

Broader row/column operations, structural typing, keyboard/selection, complex
and nested content, RTL/IME, print/export, and release performance remain
open alongside the inherited specification and audit requirements. Native
correctness captures are not performance evidence. The existing worktree and
its unrelated modifications were preserved; nothing was committed.
