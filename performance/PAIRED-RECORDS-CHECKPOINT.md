# Paired record layouts — September 10

## Layout-first outcome

Independent entity directories that overflow their available width can now use
two complete, source-ordered records per band. Each record must have at least
560 logical pixels and fit every measured value with the record insets. The
existing semantic classifier, explicit-width exclusions and comparison-table
guards remain in force. Narrower windows and enlarged text use a single record
column when it fits; otherwise the existing scrollable table remains available.

Records retain natural heights. The next band starts after the taller record,
with a 16 px gap; an odd final record keeps the same column width. This avoids
reordering source content to fill holes. Label rails are measured against each
record's own width, not the entire document. Original schema headers remain
visible and editable in a compact measured row. Header-specific width fitting
is a row-aware API; ordinary data-column fitting retains its existing contract.

Fixture122 previously squeezed the Review bundle purpose into a 108 px column,
315 px high, while identifiers extended offscreen. Its final purpose is 42 px
high. Two 649 px records and a 16 px gap use the entire 1314 px document canvas;
the following section moves up **333 px**. The numerical comparison beneath
the records retains exactly the same cell x positions, widths and heights.
No source content, identity, empty field or schema relationship is invented.

## Verification

Three new Rust tests cover paired/odd-record geometry, measured fit and stacking
at 100/150/200% mock text, comparison exclusion, complete text/source retention,
and focused body growth moving the complete following band with exact Undo.
The original short-status data-width test caught a first implementation mixing
schema and data widths; the row-aware API corrects that regression without
weakening the test. Mock text tests are not native glyph evidence.

`scripts/check.sh` passes formatting, locked all-target checking, strict Clippy,
workspace and adapter tests, and doctests: **768 passing Rust tests**, two
existing ignored tests. Log: `/tmp/mineral-paired-records-check-final.log`.
All 24 record-checker Python unit tests and `git diff --check` pass.

Immutable final layout-validation binary:
`/tmp/mineral-paired-records.jtFREc/mineral-final`, SHA-256
`f07b9a1f0f953aff1baa0da26e23ba74918ce8de3c7e6fbb317dc57feaf0c67d`.
Fixture `layout-fixtures/122-paired-records.md` SHA-256:
`c3ae9753a2fdec364181e33a95d8364cf8311ec09deeb22ea3c86de8a13d0e35`.

Native captures use isolated Weston and a private active AT-SPI session, never
the physical desktop. Prefixes under `layout-previews/`:

- `paired-records-before`: pre-change 1600×2200 light baseline.
- `paired-records-final-wide`: final 1600×2200 light, 100% text.
- `paired-records-final-narrow`: final 900×2400 light, 100% text.
- `paired-records-final-enlarged`: final 1600×2600 dark, 200% text.
- `paired-records-final-body-growth`: 1032-byte native paste into the second
  record's purpose; following band moves after the grown record.
- `paired-records-final-header-edit`: native Steward header extension by
  ` and review contact`; repeated labels update after blur.

`paired_records_check.py` passes all **88 canonical nodes**, exact source,
source-order ownership, paired/stacked bounds, full canvas utilization, retained
empty Steward and unchanged numerical comparison geometry. Four edge samples
per record match the exact rule color. The pre-change baseline supplied as the
final capture fails the alignment oracle. Wide, narrow, dark enlarged and both
post-blur edit screenshots were inspected at original size. Both native edit
cases pass whole-file exact autosave and exact Undo, with idle and blur reflow.
An earlier body-edit attempt moved into the next cell through an extra Down
key; its exact-source oracle rejected the wrong target. That failed intermediate
capture is retained and is not accepted evidence.

## Compatibility finding and remaining scope

Follow-up: `RECORD-RESIZE-CHECKPOINT.md` resolves the fixture76 mismatch below
as the changed content width after outer-margin reduction. It separately fixes
active trailing-cell visibility and premature horizontal-scroll clamping during
resize. The historical failed specimen and its original oracle remain intact.

Fixture76's old 600×2400 entity-record oracle currently fails: its first title
is a 42 px ordinary table cell rather than a 24 px record title. This is also
reproduced on the immutable pre-change fb0b1381 runtime. The before/after native
captures retain all **104 canonical nodes**, identical source and identical
header/data-cell geometry. Prefixes: `paired-records-entity-baseline-narrow`
and `paired-records-entity-control-narrow`. Thus this change did not introduce
the mismatch, but the historical entity-record acceptance is **not currently
requalified**. Investigating why that specimen no longer selects its expected
record layout remains open; its existing oracle has not been relaxed.
The additional 768×1800 `paired-records-entity-control` is a source-preservation
capture only, not a passing instance of that size-specific oracle.

T03/A07 remain partial and active. Continuous native resize/edit roundtrips for
paired records, structural edits, keyboard traversal, RTL/IME, nested records,
transposed comparisons and print variants remain unqualified. Full grammar,
media, retained accessibility and release performance remain separate work.
The app-UX skill guided readable width and source-ordered grouping; the Rust
skill guided measured layout ownership and regression checks. The debugging
skill guided the failed width-contract and native input/control investigations.

Crusty validation `task_587880a8f3a529b4` for `ctx_55069df9fa9d` completed with
75 existing advisory findings and zero new, worsened or resolved findings.
