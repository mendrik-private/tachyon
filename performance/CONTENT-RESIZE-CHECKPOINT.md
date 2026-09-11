# Content-first resize recovery — September 10

## Contract before runtime changes

Continue the full audit with layout first. A section heading that stays full
width above both columns is not itself rearranged when the table/code and
following prose switch between a stack and a pair. Its caret must therefore
not hold those independent columns in a size-forced stack after space returns.
The same distinction applies to complete figure-first explanations. Only
candidate forms that keep that heading outside every column may bypass this
lock. Other compositions, component/prose focus and actual fit guards remain.

Reproduce the current content-first failure in the mock geometry regression
and in a native width/height sequence before changing runtime logic. Reuse
the source-backed row plan, not an extra state field or a font/width workaround.
Retain current heading identity, reading anchor, selection and source bytes.
Narrow/short layouts stack; sufficient width/height recovers a legal pair.
Active prose/component edits keep their legal geometry until released.

Extend the existing native resize oracle with explicit table/code-first
fixtures and exact authored-versus-projected paragraph text. Verify original
whole-file bytes outside the edited paragraph, normalized edited paragraph
bytes and exact Undo. Native bursts are not continuous pointer-drag timing
qualification. Full grammar, export, arbitrary reading anchors and performance
remain open.

## Reproduction and cause

The extended `content_led_focus_keeps_columns_and_defers_resize_recovery_until_blur`
test first fails with the unchanged full-width heading keeping the recovered
layout stacked. The native `content-resize-before` reading sequence confirms
it on fixture116: editor widths 1040 → 650 → 1040, final committed dimensions
correct, stable identities and heading anchor, but no pair on either wide
surface. Detailed traces reject the candidates with `EDIT_LOCK`. This
distinguishes the lock from stale viewport geometry or a genuine fit failure.

The old exception only recognized `Explanation` inside one unit. Content-first
and figure-first explanations span two units, even though their heading also
paints above both columns. `keeps_heading_outside_columns` now checks the
source-root/column ranges directly. A leading stack heading or a shared prefix
outside every column can remain fixed while its columns change; a heading
inside a peer track still owns and locks its arrangement. Only candidates
containing that heading need the exception check; independent following prose
stacks are not rejected merely because they belonged to the previous pair.

The fit, overflow, short-viewport and actual prose/component editing gates
are unchanged. No new state field, font token, width workaround or source
rewrite was introduced. The existing one-unit explanation exception is
replaced, not duplicated for a growing list of presentation families.

## Verification

Runtime SHA-256:
`c843a87868574c406653af6efe109e20b578540af75f8b9f068ae5081172a98b`.
Baseline runtime: `dbce54a2…d4e89`.
Fixture116 SHA-256:
`0998f8d175a10d1707cec3e15d9bb66210abf154301d9db3d64da73436e995ca`.
Fixture117 SHA-256:
`9b6f2ba2d46b49a4d31c503c20b5c23999df78217b1c7421ba4ce2d37a3d96a6`.
Fixture117 provides a complete compact property table for the height gate;
fixture115's sixteen-row table is intentionally too tall for this harness's
604/884px editor heights. The original long table was not shortened in place.

Native final prefixes under `layout-previews/content-resize-`:

- `code-width`, `table-width`: width bursts, pair → stack → pair at
  1040 → 650 → 1040 logical editor pixels. Duplicate size, source, heading and
  component identities, zero observed heading-anchor displacement and latest
  committed dimensions all pass.
- `table-height`: height bursts, pair → stack → pair at 884 → 304 → 884px
  with 1040px width unchanged; the same identity/source/anchor checks pass.
- `code-edit`, `table-edit`: native paragraph insertion followed by width and
  height bursts respectively. Editor/caret/focus remain stable, deliberate
  size pressure stacks the content, whole-file autosave matches the explicit
  expected bytes, and Undo restores the exact original source. Both isolated
  sessions log a broken pipe during teardown after passing and exit zero.
- `explanation-control`: existing explanation-first fixture106 retains native
  width-burst pair/stack/pair behavior and all source/identity/anchor checks.
- `table-kiosk`, `code-kiosk`: separate 1360×1200 light/dark full-window
  captures pass source and appearance checks. Native pixels were inspected,
  along with the edited narrow code/prose window. Resize runs use private
  desktop-shell windows, not maximized screenshot coordinates; their semantic
  geometry/anchor checks do not qualify every window placement or palette.

The native oracle now retains separate exact authored paragraph text for the
new fixtures' soft source breaks. Accessible text uses spaces; the complete
saved-file expectation permits ordinary punctuation normalization only in
that edited paragraph. All surrounding bytes must remain exact. A new Python
test covers both fixtures and rejects appended or unrelated changed bytes.
The resize oracle passes 13 tests; capture harness passes 17 tests.

The regression now passes for both content-first source variants. A new Rust
test checks the prefix predicate across explanation/content/figure/aside/
gallery forms and rejects headings inside a column or a different source
root. Existing peer, focused-prose and editing tests remain in the suite.
`scripts/check.sh` passes formatting, locked all-target checks, strict Clippy,
**738 Rust tests (two existing ignored)** and doctests. Log:
`/tmp/tachyon-content-resize-check.log`.

The UX skill informed separating heading focus from independent content;
the diagnosis skill required unit/native red-green evidence and competing
lock/fit/stale-geometry checks. Full family/state/RTL, arbitrary mid-paragraph
anchors, continuous pointer-drag latency, paged/export and performance
qualification remain open. Native bursts do not prove those broader claims.

Crusty validation `task_d781db7cc9b40195` against `ctx_1800cc3de5db` completes
with 37 existing architecture findings and none new, worsened or resolved.
The full A07/audit goal remains active.
