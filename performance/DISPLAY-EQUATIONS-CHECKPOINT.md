# Display equations checkpoint — September 9

This advances T07 and board 04; it does not sign off the complete math family
or the full design grammar. The written grammar, not raster lettering, governs.

## Implemented presentation

Supported dollar-delimited and `math`-fenced display equations now occupy one
source-owned atomic visual row. The existing deterministic STIX/LaTeX renderer
supplies the glyph outlines and dimensions; the existing MathML/AT-SPI tree
supplies mathematical structure. Source is neither rewritten nor duplicated
on the reading surface. No equation numbers, labels or definitions are invented.

Compact equations center inside the loaded-font prose measure. Long formulas
may use the wider canvas; if they still exceed it they retain their actual
type size and pan horizontally. The scrollbar occupies a separate 16 logical
px rail below the glyphs. Equation/prose gaps are 24 px, and following major
section gaps remain 64 px. These scale to 48/128 px at 200% text. Valid reading
equations have no code-panel background, border, source header or Copy button.

Clicking a fitting equation, or focusing its native region and tapping Enter,
opens the existing canonical source-editing presentation. Overflowing equations
retain Left/Right/Home/End panning; Enter edits and Escape returns focus to the
document. The caret begins at the first source token rather than a retained
opening newline. Leaving the formula restores its reading presentation.
Incomplete or unsupported input retains its editable source and does not
fabricate a rendered result. Presentation state is not saved in Markdown.

## Regressions and diagnosis

Five new tests cover atomic source ranges, real glyph/semantic preparation,
malformed-input recovery, measured spans and 24/64 px gaps, pointer editing,
zoom, exact undo, and the source-run/native-shaper contract. Existing math
tests continue to exercise overflow, keyboard panning, denominator bounds,
cache eviction, retained previews and source-node refresh.

The first release candidate (`ba433980…ffdd3`) crashed in the actual native
shaper: syntax highlighting emitted source-byte runs into an empty display
string. Weston apps 43 and 44 reproduced it with the full fixture and a minimal
`$$ x $$` document. The new regression failed with three run bytes versus zero
display bytes. The fix makes empty display strings produce no source runs;
the final native build and the regression both pass. No dependency or unsafe
code was added. This was not caught by GPUI's test text backend alone.

`scripts/check.sh` passes locked metadata/check, formatting, warning-denying
Clippy, workspace tests and doctests. Results are in
`layout-previews/equation-grammar-checks.log`: **556 tests pass**, with two
existing ignored view tests. `equation_grammar_check.py` compiles successfully.
Crusty context: `ctx_3e7d019513f9`; validation reports 36 existing advisory
findings and no new or worsened findings.

## Native evidence

Final binary SHA-256:
`cb05beb830bc38588d6bb83cda02d7e5f55ad4606bbac51aac88afd402cb2fc1`.
Fixture `72-display-equations.md` SHA-256:
`b7f0688c9a8072a72501abd40b6b70be740a53a6e39cf63293e64cd8df455ce6`.
Immutable tested binary:
`/tmp/mineral-equation-weston.1I2Mue/tachyon-final`.

Personally inspected final native screenshots under `layout-previews/`:

- `equation-grammar-wide.png`: 1600×1800; five full rendered equations and the
  recoverable incomplete expression. Compact allocated measure is 548 px;
  the long expression uses the 1280 px canvas.
- `equation-grammar-narrow.png`: 600×2400; all five equations, collapsed
  navigation, and a visible horizontal scrollbar for the wide expression.
- `equation-grammar-200.png`: 1600×2400 at 200%; four complete equations are
  visible and pixel-checked. The remaining content stays below the viewport.
- `equation-grammar-short.png`: 1150×600; the first complete equation is
  visible, with every offscreen source/semantic relationship retained.
- `equation-grammar-edit-inside-typed.png`: native `yx = ax + b` editing,
  with the remaining equations still rendered in their reading presentation.

Each size capture has matching `.source.json`, `.active-atspi.json` and
`.pixels.json` evidence. Run:

```sh
python3 performance/equation_grammar_check.py performance/layout-previews/equation-grammar-wide performance/layout-previews/equation-grammar-narrow performance/layout-previews/equation-grammar-200 performance/layout-previews/equation-grammar-short
```

All four pass. The checker independently examines source/build identity,
native source order and mathematical roles, 24/64 px gaps, shared prose width,
actual centered glyph ink and paper corners without a code-panel tint. It is
not a renderer-derived golden. The prior `4ff76f75` bibliography release in
`equation-grammar-before.png` fails the same oracle: its first formula starts
107 px after the preceding paragraph instead of 24 px. That baseline was also
opened and inspected.

Actual Weston MCP app 46 verifies programmatic focus, real Enter press/release,
source-mode activation and outline return. `equation-grammar.weston.json`
records before/edit/after native bounds. Reading widths/heights and relative
placements restore within one pixel; outline navigation translates the whole
document by -8 px, so screen coordinates are not claimed identical. Source
hashes remain unchanged. An earlier test accidentally held Enter in app 45,
triggering normal key repeat; that private fixture was retained separately as
`72-held-enter.md` and is not evidence for single-key activation.

`equation-grammar-edit-inside.edit.json` passes real typing/autosave, named
unchanged sibling fragments and exact original bytes after undo. The initial
`equation-grammar-edit` attempt inserted at the marker's beginning, which the
strict interior-insertion oracle rejected; the final check moves Right once
before typing. `equation-grammar-copy.copy.json` verifies seven ordered source
markers including mathematical source. These are not full edited-file or
rich-MIME interoperability proofs; their exact-result fields remain null.

## Scrolling evidence

The exact final binary completed a 60.022-second continuous native run on the
generated 10 MiB mixed document: **108.86 fps**, presentation p50/p95/p99
9.13/10.64/11.87 ms, and draw p50/p95/p99 5.37/6.69/7.51 ms. Three presentation
intervals reached 25 ms (maximum 42.47 ms); no draw reached 25 ms (maximum
11.06 ms). The >60 fps gate passes, not a tail-free or 120 fps claim.
`layout-previews/equation-grammar-10mib.json` records the full protocol and hash.
Accessibility was inactive. An unrelated Cargo/rustc build was observed at
benchmark launch and absent at the next process check; this is not a pristine
idle-machine comparison or proof of performance improvement. Grammar-heavy
math and AT-SPI-active workloads remain separate outstanding checks.

## Remaining scope

Equation-reference resolution and explicit numbering placement, all supported
TeX environments/macros, deeper/nested/table math, large/RTL/mixed-script
interaction matrices and actual selectable semantic math in exports remain
unverified or incomplete. Source editing still uses the existing source panel;
this is not a structured visual mathematics editor. The renderer's existing
complexity/dimension limits still apply with a source fallback. All unrelated
open grammar, page-master, pagination, media and navigation requirements remain
in the coverage ledger and the persistent goal remains active.
