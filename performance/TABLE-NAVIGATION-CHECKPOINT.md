# Continued record editing and visible keyboard navigation — September 10

## Finding and correction

Tab/Shift+Tab selected the right canonical table cell but did not reveal its
caret. A focused three-column table with explicit 400 px columns in a 500 px
window reproduces the problem without the record classifier or a resize.
`table_keyboard_navigation_reveals_overflowing_cells` exercises the actual
forward/backward editor actions, verifies each selected source offset and its
painted caret bounds, and requires unchanged Markdown throughout.

The test failed in 0.03 seconds after compilation, with “Tab must reveal the
selected cell, not leave the caret offscreen”
(`/tmp/mineral-table-navigation-red.log`). Ranked hypotheses were missing
reveal, stale layout focus, and a later paint/reflow overwrite. Adding the
existing `keep_offset_visible` call after structural selection made the same
test pass. Render already synchronizes layout focus; no extra layout-lock or
projection policy was necessary. The correction uses the shared prepared
table/caret geometry, not a separate table-navigation layout.

The native before/after reproducer is stronger than the mock text-system
regression: fixture122 at 200%, width burst down and back, then Shift+Tab from
Steward to Repository. On the previous runtime `462dc128`, the source target
was correct but both before-typing and after-typing caret visibility failed.
The current runtime passes the identical check. Evidence is retained in
`layout-previews/table-continuation-before-header-width.resize.json` and
`table-continuation-z10-header-width.resize.json` respectively.

## Extended native contract

`capture-layout.py --resize-continue-edit` is restricted to fixture122, an
explicit record body/header target, and editing mode. After the existing
edit→shrink→restore checks, it now:

1. Types `y` immediately after the retained `x` edit and undoes only `y`.
2. Uses Shift+Tab to the previous source cell, types `z`, then undoes only `z`.
3. Uses Tab back to the original cell start, types `w`, then undoes only `w`.
4. Leaves the table through native Find, checks full-width recomposition,
   then undoes the original `x`, restoring every original source byte.

Every intermediate save has a complete-file golden, so selecting the wrong
cell cannot pass just because a caret moved. Every trial checks the caret
before and after typing, one-character selection advancement, focus, complete
record geometry, and all 88 canonical nodes. Only the exact edited cell and
its paragraph may change their accessible names. Paths, parent relationships,
roles, descriptions, actions, and every unrelated name must remain identical.
Each stepwise Undo restores the complete previous source and canonical tree.
An adversarial unit test rejects unchanged text, wrong identities, altered
actions/descriptions, unrelated edits and a cell/paragraph name mismatch.

The focused narrower measure remains stable while editing. Leaving the table
releases that measure and recomposes the records across the available canvas;
this turn does not change that previously verified interaction contract.

## Verification

Immutable native runtime:
`/tmp/mineral-table-navigation.3lcBv8/mineral-final`
SHA-256: `22847f213094d447ae12fb3ec86d23606595e3899c1754678a3e70531ff799d8`.
Fixture122 remains
`c3ae9753a2fdec364181e33a95d8364cf8311ec09deeb22ea3c86de8a13d0e35`.

`scripts/check.sh` passes formatting, locked checks, strict Clippy, 771 Rust
tests, the existing two ignored tests, adapter checks and doc tests
(`/tmp/mineral-table-navigation-check.log`). Python checks pass: 28 resize,
20 capture and 3 paired-record geometry tests. `git diff --check` is clean.

Twelve unique native cases pass: body/header × width/height × 100/150/200%.
Accepted prefixes are `layout-previews/table-continuation-z{0,5,10}-{body,header}-{width,height}`.
All use the immutable runtime and fixture hashes above. Their 36 intermediate
edit/Undo trials pass every source, semantic, geometry, focus and caret check;
all twelve then release the focused layout and exactly undo the original edit.
The separate old-runtime negative is excluded from that accepted matrix.
The 200% Repository edit and 100% released paired layout were visually inspected.
One transient resize probe lacked the editor during accessibility-tree
replacement. Existing bounded polling recovered; the settled complete tree,
committed viewport and every subsequent semantic check remained mandatory.

Crusty validation `task_0b8ee183a1300685`, context `ctx_c6566351a8ba`, reports
75 existing advisory findings and zero new, worsened or resolved findings.

Native captures use separate private Weston seats and session buses with
synthetic fixture copies, never physical desktop input. Concurrent correctness
captures and their trace times are not performance qualification.

## Remaining scope

T03 and A07 remain partial/active. Row insertion/deletion and structural Undo,
the full keyboard/selection matrix, complex or nested table content, RTL/IME,
transposed/print variants and the release performance gate remain required.
This checkpoint does not close any whole grammar family or the full audit.
