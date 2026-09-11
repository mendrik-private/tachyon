# Explanation pair recovery

2026-09-10. A07 / STR-0003 layout-first continuation. The bounded recovery paths
below are implemented; the complete audit remains active.

An explanation and its technical content may stack when width or viewport height
is insufficient. When space returns, the complete measured pair must compete
again without a penalty for leaving that size-forced stack. Actual fit, source
order and active editing locks remain authoritative. A legal focused stack must
not rearrange merely because there is now room for a pair. No typography, source
content or reading-width constraint changes to make an otherwise invalid row fit.

Qualify repeated wide → narrow/short → wide transitions for table and code
explanations, including full loaded-font geometry and native resizing. Verify
unchanged source, stable reading identity, editing and Undo. This does not
complete arbitrary mixed-document, continuous-drag, RTL/IME, paged or performance
requirements; the full audit remains active.

## Reproduced failures and implementation

`cargo test -p document-view --locked extended_explanations_use_measured_pairs`
first failed when a 1314px explanation/table pair was narrowed to 480px and
restored. The presentation-change penalty prevented a newly legal pair from
competing. `RowKind::Explanation` now receives the same size-recovery exception
as technical peers, asides and figure-led explanations. Fit constraints remain.

The native `explanation-resize-before` sequence independently reproduced the
failure: document widths 1040 → 650 → 1040, correct final committed dimensions,
unchanged identities/source, but the table stayed below the prose.

Initial native runs after the penalty-only fix still failed. Their traces
identified an edit lock: the initial caret sat in the full-width section heading,
which is part of the semantic group but outside both explanation columns. The
lock now permits only stack/explanation arrangements under that same heading;
other row kinds that could move the heading into a track remain excluded.
Actual prose/code editing still locks its arrangement.

A legal pair deferred while the prose has focus is marked provisional, using the
existing state previously limited to figure-led explanations. On blur, it can
compete again instead of making the temporary stack a permanent preference.
No additional state field, source rewrite, width cap or dependency was added.

## Automated evidence

- The extended explanation regression now covers narrow width, short height,
  restored pairs, stable duplicate dimensions, locked prose, release on blur,
  and independent full-width heading focus for table and code specimens.
- A second loaded-font test repeats width/height cycles at 100/150/200% text
  scale, requiring restoration of the entire original geometry key and unchanged
  source. The combined technical-peer specimen remains a separate control.
- The native oracle supports explicit fixtures 106/107 with paragraph/table/code
  roles, not invented heading labels. It requires horizontally disjoint peers
  with overlapping vertical bands (code headers/table insets differ), stacked
  source order, stable identities and current committed viewport dimensions.
- Twelve Python oracle tests include negatives for overlapping columns, misplaced
  technical content, replaced/missing peers, stale commits and unrelated saved
  byte changes. Seventeen capture-harness tests also pass.
- Mixed-specimen editing now checks the complete expected saved file, allowing
  ordinary Markdown escaping only within the edited plain paragraph; Undo must
  restore exact original bytes.
- `scripts/check.sh` passes format, locked all-target checks, strict Clippy,
  workspace tests and doctests: 709 passed, two existing ignored tests.
  Log: `/tmp/tachyon-explanation-resize-check.log`.

The UX skill informed independent full-width headings and content-fit recovery;
Rust and diagnosis guidance informed the source-bound lock, explicit provisional
state reuse and native/unit red-green loop.

## Native evidence

Runtime SHA-256:
`156af6f669146ddf9f28ca5200c5f42b5ccbdd379511bcb0339012ec213a6667`.
Fixture 106 SHA-256:
`664ad8ec5bd1c46f835970ca69104ec5434ae7b11656a8e6a0d6f7863d5b1fcf`.
Fixture 107 SHA-256:
`69884696d12c819489fc9b2a3d2adc439888985236c8340ce376e4dea9dad6b3`.

Final prefixes under `layout-previews/explanation-resize-`:

| Prefix | Evidence |
| --- | --- |
| `table-final`, `code-final` | Width bursts: paired → stacked → paired at 1040 → 650 → 1040 logical editor pixels. Duplicate resize stable, source and peer/heading identities unchanged, zero observed heading-anchor displacement, latest committed viewport matches. |
| `height-final` | Table explanation, 1040px width fixed; 884 → 304 → 884px height bursts restore the pair, with the same identity/source/anchor checks. |
| `editing-final` | Code explanation, native insertion then width burst. Stable caret/focus, correct narrow stack, complete expected autosave, exact Undo, matching final committed dimensions. |
| `peer-control` | Existing fixture79 technical peers still pass width-burst row → stack → row, identities/source/anchor and final-commit checks. |
| `table-kiosk`, `code-kiosk` | Separate maximized 1360×1000 light-table/dark-code captures inspected for complete window composition; source and appearance checks pass. Code retains its normal horizontal overflow for long literal lines. |

Resize sessions use private Weston desktop-shell windows on 1920×1400 outputs,
100% display/text scale and isolated source copies. Windows may extend off the
output: AT-SPI local geometry and committed traces prove resize behavior, not
window placement or whole-output palette/visibility. These are discrete native
resize bursts, not continuous pointer-drag latency tests. Heading-anchor checks
do not qualify arbitrary mid-paragraph anchors. The successful editing run logs
a private-session broken pipe during teardown and exits zero.

`before` uses original runtime `522a21fb…`. Intermediate `table-width`,
`code-width`, `table-height` still failed with the heading lock; `code-edit` tried
the heading-only ScrollTo helper with a code label. They are diagnostic captures,
not passing evidence. The final helper navigates the actual section heading
before finding the editable prose.

Crusty context `ctx_e39285f53720`. Validation and A07 evidence retain the full
remaining scope; this checkpoint does not sign off the whole grammar or audit.
