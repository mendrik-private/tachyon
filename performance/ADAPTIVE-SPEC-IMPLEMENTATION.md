# Adaptive composition implementation ledger

Contract: `/home/mendrik/Downloads/ADAPTIVE_MARKDOWN_LAYOUT_SPEC.md` (§1–20).
Additional requested deliverables: relationship-aware element gaps, Blitz HTML
fragment rendering with simple text-node editing / rich-text-to-Markdown
conversion, and rendered mathematical formulas. This ledger does **not** narrow
the contract. Existing heuristic grids are not the completed measured planner.

## Integration and baseline (WP-01)

Tachyon is a native Rust/GPUI Wayland document editor. The intended visual
language remains warm light, Fraunces headings, Spline Sans body at 18 logical
pixels, and Spline Sans Mono code. The document is the work surface; there is no
replacement app shell or web editor. Files and Outline remain the existing panes.

| Responsibility | Concrete owner / current evidence |
| --- | --- |
| Canonical content, IDs, revisions | `document-core/src/model.rs`, `document.rs`; persistent BlockSequence, NodeId, immutable snapshots |
| Markdown/GFM import and source preservation | `document-core/src/markdown.rs`, source spine; Comrak parser, transactional serialization |
| HTML import | `document-core/src/html.rs`; html5ever conversion, inert dangerous-descendant removal; unsupported source retained |
| Transactions, undo, selection | `document-core/src/command.rs`, `document.rs`; document-view session and editor input handler |
| Read-only renderer adapter | `document-view/src/projection.rs`; projected text plus canonical root IDs, leaf contexts and table constraints |
| Group/section analysis | `document-view/src/adaptive/groups.rs`; canonical root intervals, descendant ownership, real heading levels and relationship basis IDs |
| Existing presentation choice | `document-view/src/adaptive.rs`; measured internal lists in `adaptive/candidates.rs`, bounded measured peer/explanation/gallery rows in `adaptive/rows.rs`; initial lists use semantic vertical treatments until measured |
| Geometry and gaps | `document-view/src/editor/arrangement.rs`; shared editor lines and component bounds |
| Text shaping | `document-view/src/editor/measurement.rs` uses GPUI offscreen shaping with the same inline font runs as painting. Background reflow and incremental edits use measured wraps; initial font-free preparation retains the estimated fallback. |
| Tables | `editor/measurement.rs` shapes complete bounded header/cell sets; projection owns minimum/preferred constraints and fitting; nested flow preserves cell geometry and local overflow |
| Image dimensions/policy | `markdown-app/src/image_cache.rs`; node dimensions passed to document-view; reuse this owner, do not fetch during measurement |
| Focus, IME, anchors, commit | `document-view/src/editor.rs`, `adaptive/rows.rs`; per-view focus-aware row/width locks, canonical range remapping, selection/IME commit guards and source-offset scroll anchors; full native acceptance matrix remains |
| Native accessibility | `editor/accessibility.rs` publishes cached canonical AccessKit nodes independently of painting; private AT-SPI checks exercise offscreen traversal/reveal, task actions/undo, and contextual source-order semantics for visually composed tables. Complete text, inline-link, HTML and adapter-level table-interface acceptance remains. |
| UI preference persistence | `markdown-app/src/persistence.rs`; workspace/document identity, separate from Markdown; no layout override schema yet |
| Performance instrumentation | `markdown-app/src/performance.rs`, `performance/capture-layout.py`, isolated Weston harness |
| Fixtures/design references | `performance/layout-fixtures/`, `designs/`; user-authored Nudge document must remain a read-only external fixture if found |

Baseline: `performance/ADAPTIVE-LAYOUTS.md` and `MOMENTUM-SCROLLING.md`
describe prior behavior and measurements, not completion of this contract.
The pre-spec workspace check passed 168 tests. Native wheel/pixel coast tests
pass on the existing release. New composition measurements must be reported
separately; no performance target is inferred from a green correctness test.

## Work packages and remaining work

| Package | Current state / required next work |
| --- | --- |
| WP-01 | Integration mapped above. Existing synthetic fixtures available; full AT-specific fixtures and source-order baseline still to extend. |
| WP-02 | Canonical groups and heading tree integrated into measured planning with conservative initial stacks. Full acceptance still requires identity/ownership, barrier, duplicate-heading and generated-partition evidence at the requested scope. Hierarchical adaptations use the same IDs. |
| WP-03 | **Partial:** native shaping supplies line breaks, intrinsic widths, measured list/peer eligibility and complete bounded table constraints. Wrap cache includes node/revision/local range, exact text, font runs, width and scaled font size; intrinsic cache keys exact text, font runs and scaled size. Font-family/zoom changes invalidate geometry. Image-resource arrivals now have bounded batching with anchor preservation. Candidate measurement is bounded to visible/lookahead windows and retains completed compatible windows; whole-document geometry publication remains the dominant long-document cost. Still required: font-completion notifications and complete pending-resource integration. |
| WP-04 | **Partial integrated row planner:** 40-unit DP with previous-template state, contiguous 1–3-unit rows, all seven track templates, measured peer/explanation/table/gallery rows, normalized scoring, deterministic ties, 10% hysteresis and invalid-layout escape. Gallery internal figures use bounded units without changing canonical groups. Visible/lookahead and structurally affected windows are scheduled without remeasuring intervening chapters. The opt-in developer trace now reports stable, content-free nomination/fit/rejection reason codes for row and list candidates. Still required: full resource-state integration, broader inspector/acceptance verification and the full edit-lock/anchor cost contract. |
| WP-05 | **Partial:** stack/list grids/measured peer cards, measured explanation/code/table/image pairs, unequal adjacent table groups and measured galleries. No font shrinking to force fit. Private AT-SPI now verifies source-order table/row/header/cell traversal and contextual names while adjacent tables are visibly composed side by side; broader native semantics remain unfinished. |
| WP-06 | **Partial:** focused row/list/prose width locks, native group-blur reconsideration, identity-remapped placements, focus/selection/IME commit guards and source-offset anchors. Full structural-edit/IME/AT-SPI and resize/anchor acceptance still required. The user's later explicit direction supersedes §15's user-selectable modes and group overrides: layout remains automatic-only, with no layout chooser or Markdown annotations. Internal stack fallback remains available for safety. |
| WP-07 | **Incomplete:** full acceptance matrix, feature/mode fallback, native visual/authoring/accessibility verification, measured planning budgets and performance evidence. |
| HTML | **Partial:** actual Blitz local layout/paint, bounded inert input, retained preview, source-bound native disclosure controls, temporary direct text selection, atomic conversion on first edit and provisional same-leaf IME, plus explicit undoable conversion. Verified-text links have safe web/email opening, native heading/local Markdown navigation, hover and context copy. Opaque links retain exact Blitz pointer geometry without gaining editable text ranges or placing destinations in the renderer DOM. Authored IDs in supported previews resolve with nested-disclosure reveal and verified target carets. Still required: full native CJK IME/candidate UI, cross-fragment selection, find/source-only anchor reveal, full semantic tree/AT, safe resource integration and complete performance/scale gates. |
| Math | **Partial:** real `latex-rust` / STIX block and inline formulas, measured inline attachments with source-indexed caret/copy geometry, per-paragraph source reveal while editing, matrix/script fixes, canonical preservation, native MathML/token validation and accessible overflow ranges. Overflowing block formulas are pointer- and Tab-focusable and support Left/Right/Home/End/Escape without changing source. Safe unwrapped LTR-leading formulas can precede an RTL suffix. Still required: cross-platform assistive-technology validation, fully bidirectional RTL-first/wrapped object mapping and the complete formula quality matrix. |
| Gaps | Pairwise heading/prose/group spacing and external-vs-internal geometry are implemented; native fixture reviews and edit-lock title/intro gap regression pass. Full content/viewport acceptance remains. |

## Current implementation evidence

### Contextual semantics for automatically composed tables

The canonical accessibility tree previously named every Markdown table only
`Table`, even when an authored section heading provided a concise, stable
description. The section-aware semantic constructor is now shared by unit-test
and cached production publication paths. A table inherits its nearest preceding
authored heading as `<heading> table`; its source, identity and visual placement
remain unchanged. Cells additionally publish explicit unit row/column spans,
matching GFM's non-spanning table model.

Release `caff09e469d924ddfbe346a37d1cf5a4bedd584df5bb754fc22342922919c5ff`
passes the isolated private-session AT-SPI fixture at 1440×1000. The two tables
are visibly side by side, while traversal remains canonical table → row →
column-header/cell order and announces `Connection properties table` followed
by `Execution options table`. Offscreen heading reveal, task activation, exact
undo, stable heading identities and formula discovery also pass. The fixture is
byte-identical after the exercise. Evidence: `spec-adaptive-tables-at.png`,
`.atspi.json` and `.source.json` in `layout-previews/`. The current AccessKit
Unix adapter exposes these roles through Accessible/Component rather than a
separate AT-SPI Table interface, so adapter-level table navigation remains an
explicit acceptance gap.

### Explainable automatic layout choices

The opt-in layout trace previously exposed normalized score terms and rejection
counts, but not the semantic reason a candidate existed or the specific fit
decision attached to it. That made the automatic-only product direction hard to
tune without reconstructing planner state from geometry. Trace schema 2 now
adds deterministic, content-free `reason_codes` to chosen rows, rejected row
candidates, list decisions and list candidates. Codes cover structural
nomination (`COMPACT_SIBLING_SECTIONS`, `ADJACENT_EXPLANATION`,
`ADJACENT_TABLES`, `CONSECUTIVE_IMAGES`, `SHORT_FLAT_LIST`), measured fit,
unequal tracks, conservative stack fallback, resource/edit locks, retained
layouts and each hard rejection. They contain no Markdown text, paths, URLs or
formula/HTML source and remain behind the existing developer trace shortcut;
no user layout chooser was introduced.

Release `5f1d18da8e901998f925b6a0aba43e336d59b70a097924b51fbd312b349970f9`
was exercised through isolated native Wayland at 1920×1080. Fixture 11 reports
the visible three-column sibling-section row and unequal explanation/code pair;
fixture 6 reports its measured three-column list plus long-item rejection. The
explicit warm planner samples complete in 0.124–0.234 ms for fixture 11 and
0.147–0.202 ms for fixture 6; both source oracles are byte-identical. Evidence:
`spec-auto-reasons-1920.*` and `spec-auto-list-reasons-1920.*` in
`layout-previews/`. These small warm samples validate the diagnostic path, not
the specification's cold long-document planning budget.

The same binary passes the 10 MiB isolated continuous-scroll gate at **109.4
FPS and 5.70 ms draw p99**, with byte-identical generated source. Evidence:
`spec-auto-reasons-scroll.json` and `.source.json`. This preserves the user's
over-60-FPS scrolling requirement while expanding only off-frame diagnostics.
The full workspace passes **432 tests**, with two installed-font tests ignored;
17 Python harness tests, formatting, diff whitespace checks and warning-denied
all-target workspace Clippy pass.

### Inert link geometry for opaque HTML

HTML structures that Blitz can render but the conservative converter cannot
map back to editable Markdown—such as a table with spanning cells—previously
lost all link interaction. The inert adapter now assigns authored anchors a
generated numeric descriptor and retains their destinations in a separate
side channel. Live `href` values still never enter Blitz's DOM or its denying
resource provider, and a caller-authored descriptor cannot spoof the generated
mapping.

For fragments without verified text correspondence, the view accepts only
finite, in-viewport Blitz client rectangles whose centres hit the same anchor;
transformed, hidden, occluded and empty boxes are rejected. These regions allow
the existing safe Ctrl-click and context-copy paths while retaining the exact
source as opaque HTML. They deliberately have no editable byte range, so a
plain click cannot imply a source mapping or trigger conversion. Focused core
and view regressions cover descriptor isolation, unsafe-scheme inertness,
spanning-table geometry, source identity and absence of invented edit ranges.
The full workspace passes **431 tests**, with two installed-font tests ignored;
17 Python harness tests, formatting, diff whitespace checks and warning-denied
Clippy for both affected crates pass. This is renderer-level geometry evidence;
native assistive-technology semantics for opaque links remain part of the
unfinished HTML acceptance matrix.

### Keyboard-operable display-formula overflow

The visible display-math scrollbar and native accessibility value previously
left a keyboard-only reader without a direct focus target. Overflowing formula
viewports now enter the native tab order and expose a restrained inset focus
ring plus a short interaction hint. Left and Right pan by the same bounded
viewport-relative step as accessibility actions, Home and End reach the exact
edges, and Escape returns focus to the Markdown editor. Fitting formulas do not
gain a tab stop. Tab keeps its existing table-cell and list-indentation behavior
inside those structures; from ordinary prose it now follows the native control
order instead of only ringing the system bell.

The focused GPUI regression exercises both pointer and Tab entry, every formula
navigation key, Escape, exact serialization, unchanged revision and an empty
content-undo history. The full workspace passes **428 tests**, with two
installed-font tests ignored. Release
`ffaec5e8fb45edb789cf69e448771bd269b1e29b9c8a63345e613f6ac1e19534`
was exercised through the private Wayland seat at 360×1000. The before capture
shows the formula start; the keyboard End capture shows the inset focus ring and
the final `a_17 + a_18 + a_19 + a_20 = S`. Both source reports are byte-identical
(`8e2659d3…`). Evidence: `spec-math-keyboard-before.*` and
`spec-math-keyboard-end.*` in `layout-previews/`.

The same release passes the formula-stress continuous-scroll gate at **109.8
FPS and 3.35 ms draw p99**, with byte-identical source
(`spec-math-keyboard-scroll.json` and `.source.json`). This is an isolated
Weston regression gate, not physical-display performance or complete AT-12
coverage. RTL-first/wrapped inline formula mapping and cross-platform assistive
technology remain unfinished.

### Discoverable contained overflow for display math

Long display formulas already retained their full measured width, but the
clipped viewport had no visible affordance. At 360 logical pixels and 200% text,
fixture 25 stopped visibly near `a_3` even though horizontal pointer input could
reach later terms. The before/end captures are
`spec-math-overflow-narrow-before.png` and
`spec-math-overflow-narrow-end.png` in `layout-previews/`.

Each display-math node now owns a stable native horizontal `ScrollHandle`.
When—and only when—the measured formula is wider than its viewport, the editor
places the existing GPUI component scrollbar immediately below the glyph
surface, inside the already-reserved preview/source gap. It does not shrink the
formula or cover fraction denominators. Handles survive ordinary redraw/reflow,
are keyed by canonical node identity, and are pruned when a formula is deleted
or a document is replaced. The scrollbar remains presentation state: dragging
it does not edit Markdown or add undo history.

The focused regression was red before the affordance existed. It now checks
0.75×, 1× and 2× text scales, overflow geometry, persistent end position, final
glyph reachability, exact source preservation, no content undo, no scrollbar
when the formula fits, and state removal after canonical block deletion. All
**423 workspace tests pass**, with two installed-font tests ignored; 17 Python
harness tests, formatting, diff whitespace checks and warning-denied workspace
Clippy pass.

Release `d61a081ff990c1e9c310a5f15b9f66ce9d5dc96c40862d6f184a19f8c2078184`
was reviewed at 360×1400/200% and 768×1000/100%. A real native thumb drag at the
narrow size reaches the final `a_19 + a_20 = S`; the corresponding source oracle
is byte-identical (`052bfdec…` before and after). Evidence:
`spec-math-scrollbar-narrow.png`, `spec-math-scrollbar-narrow-drag.png`,
`spec-math-scrollbar-medium.png`, and their `.source.json` reports.

Overflow state is now operable through native accessibility as well. The
canonical formula node keeps its pure MathML child tree and exact source label;
only an overflowing formula additionally publishes standard horizontal-scroll
offsets plus a native numeric Value range. AccessKit scroll actions and AT-SPI
Value changes both call one clamped per-node view-state path. Fitting and invalid
formulas expose neither a false range nor an extra semantic child. Release
`5cbd850970bd715fd1090d44514fbe217da91e4f6cb82d166e6902efb710de35`
passes the private AT-SPI exercise at 360×1400: the long formula moves from 0 to
its exact maximum 803, its final `= S` is visible, all structured equation/token
checks still pass, and source remains byte-identical (`8e2659d3…`). Evidence:
`spec-math-scroll-accessible.png`, `.math-atspi.json`, and `.source.json`.
The same private AT-SPI exercise passes at 200% text in a 768×1600 window: the
range expands from 0 to 1,503, the final `= S` remains reachable, equation
structure and token order remain intact, and source is unchanged
(`spec-math-scroll-accessible-200.*`).
With accessibility active, the same release passes the formula-stress
continuous-scroll gate at **104.7 FPS and 12.08 ms draw p99**, with no interval
or application stall at least 25 ms and byte-identical source
(`spec-math-scroll-accessible-perf.json` and `.source.json`).

The same release passes the formula-heavy isolated Wayland continuous-scroll
gate with an active private AT-SPI tree: **108.4 FPS, 9.26 ms draw p99**, no
application or presentation interval at least 25 ms, and 2,473 accessible nodes
over ten measured seconds (`spec-math-scrollbar-formula-scroll.json`). The
adjacent `.source.json` report records byte-identical formula-stress source
(`0381ff2b…`). This is a
regression gate on the private compositor, not physical-display performance or
a claimed speedup. Direct non-AT keyboard focus for the horizontal formula
viewport, complete formula semantics across assistive technologies, and broader
mixed-direction inline math remain part of the unfinished AT-12 matrix.

### Safe inline formulas before RTL suffixes

The earlier inline-math gate rejected an entire paragraph as soon as it found
an Arabic or Hebrew character. It now admits a bounded mixed-direction case:
all formula attachments must belong to a leading LTR portion, every RTL
character must follow them, native shaped glyph positions must prove that no
source span crosses the formula visually, and the complete paragraph must fit
without wrapping. This uses measured output rather than guessing from item
content. RTL-leading, interleaved and wrapped cases still render their canonical
source until fully bidirectional object/caret mapping exists.

Focused regressions cover Arabic and Hebrew in one suffix, formula/source range
identity, narrow-width fallback, RTL-leading fallback, source reveal while the
formula is edited, exact serialization and one-step undo. All **425 workspace
tests pass**, with two installed-font tests ignored; 17 Python tests, formatting,
diff whitespace checks and warning-denied Clippy pass. At 768×1200, release
`9fa764ab56b313687c4dbdb90d0a69be0efe254282d143e744ab842fe9737e51`
renders `E = mc²` before the shaped RTL suffix while the RTL-leading `x^2 + y^2`
remains source. At 360×1400 the first paragraph wraps and therefore also retains
source. Both native runs preserve the fixture byte-for-byte (`a6bd8de3…`).
Evidence: `spec-math-mixed-direction.png`,
`spec-math-mixed-direction-narrow.png`, and their `.source.json` reports.
The same release passes the formula-stress continuous-scroll gate with native
accessibility active at **109.6 FPS and 6.40 ms draw p99**, with byte-identical
source (`spec-math-mixed-direction-perf.json` and `.source.json`).

### HTML-only documents: retire the unneeded caret scaffold

An HTML-only import needs a temporary empty paragraph so the native editor has
a legal caret before conversion. It previously survived text conversion as if
authored: the minimal core regression emitted `FiXond\n\n\n`, and the native
fixture `46-html-only-editing.md` saved `Alpha x *tail*\n\n\n` instead of the
expected single trailing newline. Release `ca103a1d…` failed the full-source
native oracle (`spec-html-only-edit-before.selection.json` records the binary
and successful initial selection; the failure was the final exact-source check).

The canonical snapshot now tracks the shared identity of its import/repair-only
caret host. It is not exported while untouched. Once real editable content
exists, a content transaction may retire the unused host after resolving the
complete selection. A host used as a range endpoint remains valid until the
replacement has resolved. Any mutation promotes it to authored content—even
when the user later erases its text. No generic empty-paragraph trimming occurs.
Undo/cancel restores the original snapshot and caret identity. Ordinary documents
have no scaffold marker and take the constant-time no-op path.

Core tests cover exact conversion output, LF/CRLF/no-final-newline and authored
trailing whitespace, both selection directions, provisional Unicode candidates,
cancel/commit/undo, ranges ending in the host, authored empty paragraph retention,
and source-only structural edits that still require the temporary caret. The
existing media-conversion assertion now counts only its three real content
blocks. All **422 workspace tests pass**, two ignored; 17 Python tests, formatting,
diff whitespace checks and warning-denied workspace Clippy pass.

Release `51b71cc9ac895a52c2d265994d1685f0280489e35a4f947edaff909c5d0a7276`
passes the same native exact-output edit and one-step undo. Reverse selection
also passes native dead-key preedit, Escape restoration, restarted preedit,
commit to exactly `Alpha é *tail*\n`, and exact undo. Reviewed the typed and
restarted-preedit screenshots: only the converted text remains, with no trailing
empty editor block. Evidence: `spec-html-only-edit-after.*` and
`spec-html-only-compose-after.*` in `layout-previews/`.

```sh
python3 performance/capture-layout.py \
  --fixture 46-html-only-editing.md --width 1280 --height 700 --startup-wait 8 \
  --select 313 107 298 154 --copy-selected $'café\nBeta' \
  --edit-check --edit-expect $'Alpha x *tail*\n' \
  --output performance/layout-previews/spec-html-only-edit-after.png
```

For the composition check reverse the endpoints, add `--edit-compose-check
'Alpha ´ tail' --edit-compose-cancel-check`, and expect `Alpha é *tail*\n`.
The same release passes the isolated 10 MiB continuous-input scroll gate at
1728×1080, scale 200/120 and 120 Hz: **109.4 FPS, 4.66 ms draw p99** over ten
measured seconds after warmup (`spec-html-only-scroll-10m.json`). This is a
regression gate on the private compositor, not physical-display performance or
a measured speedup from the scaffold change.
This fixes a concrete source/spacing defect, not the complete content/viewport
spacing matrix or native CJK candidate-window acceptance.

### Native Escape cancellation across HTML–Markdown selections

The native dead-key cancellation check exposed a platform event-order defect:
the locked GPUI Wayland backend inserted the pending accent when XKB reported
`Cancelled`, before delivering Escape to the editor. On release `4c06c01…`,
Escape therefore saved `Alpha ´ tail` instead of restoring the two original HTML
fragments and intervening Markdown. The exact copied selection and unchanged
source assertions both failed; see `spec-preview-cancel-before.cancel.json` and
`spec-preview-cancel-before-cancelled.png` in `layout-previews/`.

The pinned `gpui_linux` crate is now a local patch, with its original Apache
license and provenance in `vendor/gpui_linux/README.tachyon.md`. Its source differs
from the pinned upstream only in a guarded Escape branch: discard the backend
preedit, reset XKB composition, and deliver Escape without committing text. The
editor's existing cancellation transaction restores content and selection. The
ordinary invalid printable-sequence fallback remains unchanged. No application
unsafe code or shared Cargo checkout was modified.

The native regression uses a private Weston seat, session bus, clipboard and
copied fixture. It checks preedit without autosave, cancellation after the save
interval, exact copied selection after replacing the clipboard with a sentinel,
restarted preedit, exact full-source commit and one-step undo:

```sh
python3 performance/capture-layout.py \
  --fixture 44-cross-preview-selection.md --width 1280 --height 1100 --startup-wait 8 \
  --select 313 231 298 385 --copy-selected $'café\nMiddle\nBeta' \
  --edit-check --edit-compose-check 'Alpha ´ tail' --edit-compose-cancel-check \
  --edit-expect $'# Cross preview selection\n\nBefore\n\nAlpha é *tail*\n\nAfter\n' \
  --output performance/layout-previews/spec-preview-cancel-forward.png
```

Reverse selection uses `--select 298 385 313 231`. Current verification: 417
workspace tests pass, two ignored; 32 vendored-backend tests and 17 Python tests
pass; workspace formatting and warning-denied Clippy pass. Release
`ca103a1d11ddc0e8fdf2bbf16468a534b1860d8484e4e324b2b8313e1080feea`
passes the full native sequence in both directions. The forward/reverse
`.cancel.json`, `.restart-preedit.json` and `.edit.json` reports are in
`layout-previews/`; the cancelled and restarted-preedit screenshots were visually
reviewed. Cancellation preserves the original HTML styling and cross-fragment
selection; restart shows only the provisional replacement, without an autosave.
The same release passes the isolated 10 MiB continuous-input scroll gate at
1728×1080, scale 200/120 and 120 Hz, after an eight-second startup wait: **109.6
FPS, 3.95 ms draw p99** over ten measured seconds
(`spec-preview-cancel-scroll-10m.json`). This is an isolated-compositor result,
not physical-display performance or a claim of improvement from the key fix.
This does not establish full native CJK candidate-window support or complete
AT-10/INV-09 acceptance.

### Provisional composition over cross-preview selections

The core composition lifecycle now supports source-verified selections spanning
HTML and Markdown, and selections spanning multiple converted text blocks in one
HTML fragment. `Document::begin_preview_composition` uses the existing private
preview resolver, validates replacement feasibility without publishing it, and
retains both the original snapshot and one converted baseline. Ordinary and
preview composition updates use the same canonical range replacement. Each
candidate restarts from the baseline rather than accumulating candidates or
converting again. Begin does not change current source/revision; commit creates
one history entry, and cancellation/undo restore original HTML and Markdown.
Unsupported nested replacements fail before composition ownership is acquired.

The view no longer rejects cross-preview preedit. UTF-16 replacement addresses
are resolved in the complete immutable preview selection; its recorded revision
is passed through rather than relabelled with the current revision. The first
structural preedit refreshes the complete projection, so deleted blocks cannot
remain rendered. Later same-node candidates retain the local refresh path.
Cancellation explicitly rebinds the restored HTML selection to the fresh core
revision after rebuilding the original projection; arbitrary stale reflows
still reject old ranges. Shared-session composition ownership remains enforced.

The previous view regression was changed from expecting rejection to requiring
real provisional text, successive candidates, cancellation with the original
selection, continued selection after zoom/reflow, and a complete-range commit
with exact one-step undo. It failed before implementation. Core tests cover both
selection directions, successive Japanese/emoji candidates, stable insertion
identity and unaffected sibling allocations, cancellation, commit/undo/redo,
stale revisions, invalid UTF-8/unknown targets, and ordinary-vs-preview
cross-block behavior. **417 workspace tests pass**, two existing installed-font
tests are ignored, and warning-denied workspace Clippy and formatting are clean.
All **17 Python harness tests pass**.

The native harness adds `--edit-compose-check PREEDIT_TEXT`: a private US
International dead-key sequence must publish exactly the expected provisional
text in the private AT-SPI tree while the copied source file remains unchanged
through the autosave interval. It then commits `é`, compares the entire edited
Markdown and verifies exact undo. This distinguishes real preedit from a rejected
preedit followed by an ordinary direct insertion. Old release
`4af18997edd0971b35b208b73622aeb0bd2a9b1e6b1e88c836918a796d95eef8`
fails this oracle for fixture 44's `café\nMiddle\nBeta` selection: original text
remains in the native tree instead of `Alpha ´ tail`
([before evidence](layout-previews/spec-preview-composition-before.preedit.json)).

Normal release
`4c06c01aec4c16231179ac148062e4ff0a5466470bd2e8549da40848d3e83b6a`
passes this native check in both selection directions at 1280×1100, 100% text
and compositor scale. The provisional AT-SPI text is exactly `Alpha ´ tail`,
with unchanged copied source through the 1.2-second autosave interval. Completing
the sequence produces exactly `Alpha é *tail*` between unchanged `Before` and
`After` paragraphs, with the fixture title retained. One undo restores every
original byte ([forward preedit](layout-previews/spec-preview-composition-forward.preedit.json),
[forward edit/undo](layout-previews/spec-preview-composition-forward.edit.json),
[reverse preedit](layout-previews/spec-preview-composition-reverse.preedit.json),
[reverse edit/undo](layout-previews/spec-preview-composition-reverse.edit.json)).
The forward preedit and committed screenshots were visually inspected. The
transport is native XKB US International dead-key composition on a private
Wayland seat, not a CJK candidate-window test. Cancellation is verified through
the real editor/core input methods; native Escape/candidate cancellation remains
separate coverage.

The release passes the isolated 10 MiB continuous-input scroll gate at
**108.4 FPS, 6.96 ms draw p99**, with nonzero input-latency samples,
1728×1080, scale 200/120 (166.7%), 120 Hz and ten measured seconds
([report](layout-previews/spec-preview-composition-scroll-10m.json)). This
measures ordinary scrolling on the isolated compositor, not active IME latency
or the physical desktop.

This is not full CJK candidate-window or RTL IME acceptance. Native candidate
UI, complete nested-container replacement, authored-disclosure interactions and
the broader AT-10 matrix remain open. A standalone preserved-only document also
retains its pre-existing generated empty editing paragraph after conversion;
its trailing-gap behavior is separate from these composition tests.

### Horizontal and word navigation across HTML boundaries

`editor/preview_navigation.rs` also handles Left/Right, Ctrl+Left/Right and
their Shift variants when a retained preview participates. It resolves the
current source segment and adjacent segment, not the rendered HTML placeholder
label. A plain boundary move enters the neighboring text at its edge without
consuming a glyph; a word move uses the editor's existing Unicode grapheme and
word-class rules in that neighboring text. The existing Markdown-only path is
unchanged. No new document-wide editable buffer, DOM layout or source conversion
is performed for single-caret movement; extending across a boundary uses the
existing lazy immutable selection projection.

The focused regression failed with Left trapped at the beginning of the first
HTML fragment. It now checks both crossing directions, word boundaries, UTF-8
grapheme movement, restored Markdown carets, exact source and unchanged revision.
A second test verifies forward/reverse word-selection copy order, collapse to
both real endpoints, plain Shift movement through the boundary and collapse
back into Markdown with its normal editing path. Opaque, nonconvertible HTML
is not assigned invented text positions; full traversal through opaque fallback
surfaces remains separate acceptance work. Visual bidirectional/RTL navigation
is not established by these logical source-order tests.

The native harness now accepts Left/Right and Ctrl/Shift word combinations;
its modifier test checks press/release ordering for Control and Shift. Release
`88c7a57d5f3487c7f34aae407b61c7c62fee81c14c749966ab1941f23c6b9c50`
failed five `ctrl-shift-right` moves from `(260,231)` in fixture 44: it copied
only `Alpha café`, not the requested range through `Middle` and `Beta tail`
([before report](layout-previews/spec-preview-horizontal-before.selection.json)).

The rebuilt release
`4af18997edd0971b35b208b73622aeb0bd2a9b1e6b1e88c836918a796d95eef8`
passes that exact native sequence and the reverse five `ctrl-shift-left`
sequence from `(400,385)`. Both copy `Alpha café\nMiddle\nBeta tail` exactly,
preserve source before typing, replace the complete selected range with `x`
into the expected entire Markdown, and restore every original byte with one
undo ([forward selection](layout-previews/spec-preview-horizontal-forward.selection.json),
[forward edit](layout-previews/spec-preview-horizontal-forward.edit.json),
[reverse selection](layout-previews/spec-preview-horizontal-reverse.selection.json),
[reverse edit](layout-previews/spec-preview-horizontal-reverse.edit.json)).
The forward selected-state screenshot was visually inspected for highlights
in both HTML previews and the ordinary paragraph between them. This is private
Weston input at 1280×1100 and 100% text/compositor scale on copied fixture 44.
All **415 workspace tests** pass (two existing installed-font tests ignored),
with clean warning-denied workspace Clippy, formatting and all **17 Python
harness tests** passing. This does not complete the broader native IME, RTL,
opaque-fragment, authored-disclosure and assistive-technology acceptance matrix.

The native plain-key sequence `left right end right left right` from the first
HTML start then types `xMiddle` into the adjacent Markdown paragraph. The
whole-source oracle verifies both HTML fragments remain byte-identical and one
undo restores the original file
([ordinary editing report](layout-previews/spec-preview-horizontal-markdown.edit.json)).

The same release passes the isolated 10 MiB continuous-input scrolling gate:
**106.7 FPS and 9.47 ms draw p99**, 1728×1080, scale 200/120 (166.7%),
120 Hz and ten measured seconds, with nonzero input-latency samples
([report](layout-previews/spec-preview-horizontal-scroll-10m.json)). This is
not a physical-display result or a keyboard-latency benchmark.

### Rendered-line keyboard selection across HTML and Markdown

`editor/preview_navigation.rs` now routes Up/Down and their Shift variants
through source-addressed positions when movement enters, leaves or stays in a
retained HTML preview. The existing Markdown navigator remains responsible for
ordinary text-only movement. Collapsing into Markdown restores the normal
editing/IME route instead of leaving an ordinary caret in a cross-preview
buffer. HTML movement uses retained legal grapheme caret stops and measured
rows; it does not reconstruct a DOM or convert source on a navigation key.
Home/End and the new Shift+Home/End bindings use rendered line boundaries,
including a wrapped Markdown head with an anchor inside HTML.

The earlier focused regressions exposed a trapped HTML caret and authored-
newline rather than wrapped-line Home/End behavior. Coverage now checks both
directions across adjacent blocks, all four line-boundary actions, fixed Shift
anchors, canonical source/revision preservation, and multi-row HTML containing
combining characters. **413 workspace tests pass**, with two existing installed-
font tests ignored, clean warning-denied workspace Clippy and formatting, and
**17 Python harness tests passing**. Context7's GPUI key-binding documentation
was checked against the pinned implementation's action/context conventions.

The native harness accepts bounded `--selection-keys` after a click/drag, with
explicit key and modifier release. Fixture 44 at 1280×1100, 100% text and
compositor scale, establishes the actual keyboard path:

- Before: release `3fc0ae7865e08e1604793c66d9e83a8e57859bd838e2e87bc63a9214c36169a2`
  failed `shift-down shift-down shift-end` from the first HTML line: copy was
  empty ([before report](layout-previews/spec-preview-keyboard-before.selection.json)).
- Release `88c7a57d5f3487c7f34aae407b61c7c62fee81c14c749966ab1941f23c6b9c50`
  passes that sequence from `(260,231)`, and `shift-up shift-up shift-home`
  from `(400,385)`. Both copy exactly `Alpha café\nMiddle\nBeta tail`, without
  changing source. Both replace that selection with `x` into the exact entire
  expected Markdown and restore every original byte with one undo
  ([forward selection](layout-previews/spec-preview-keyboard-down.selection.json),
  [forward edit](layout-previews/spec-preview-keyboard-down.edit.json),
  [reverse selection](layout-previews/spec-preview-keyboard-up.selection.json),
  [reverse edit](layout-previews/spec-preview-keyboard-up.edit.json)).
- Plain `down` from the first HTML caret followed by typing produces `xMiddle`
  while preserving both original HTML fragments and every other source byte;
  one undo restores the fixture
  ([ordinary editing report](layout-previews/spec-preview-keyboard-markdown.edit.json)).

The forward/reverse selected screenshots and the forward typed screenshot were
visually inspected. These checks use copied synthetic documents and private
Weston input/state, never the user's physical desktop or documents.

The same release passes the isolated 10 MiB continuous-input scroll gate at
1728×1080, compositor scale 200/120 (166.7%), 120 Hz and a ten-second measured
window: **107.4 FPS, 8.45 ms draw p99**, with nonzero input-latency samples
([performance report](layout-previews/spec-preview-keyboard-scroll-10m.json)).
This is an isolated-compositor scrolling result, not physical-display or
keyboard-latency evidence.

This completes a bounded part of INV-03/09 and HTML authoring, not full AT-09/10.
Horizontal/word traversal across fragment boundaries, full RTL and mixed-font
baseline navigation, native CJK candidate UI and multi-block composition remain
incomplete. Unpainted ordinary target rows still use a grapheme-safe approximate
x fallback until paint; exact offscreen shaping and wrap-boundary affinity need
further work. The full MVP acceptance matrix remains open.

### Viewport-edge drag selection

`editor/drag_scroll.rs` gives HTML and ordinary Markdown selection one
frame-driven autoscroll owner. It uses the visible scroll viewport, not the
full document canvas. Within the 32 logical-pixel edge band (smaller in tiny
viewports), speed follows pointer distance and is capped. Elapsed frame time
sets the displacement, with a 50 ms catch-up cap; this is direct drag control,
not decorative animation or a release coast. Each tick resolves the head from
the just-painted geometry and retains the canonical anchor. No document edit
or source conversion occurs while dragging.

GPUI's ordinary `on_mouse_move` callback is hover-only. A paint-registered,
drag-only outside-pointer handler therefore also covers jumps into the shell's
20-pixel inset. It does not process an inside event twice. One weak, generation-
guarded frame chain is retained across repeated pointer input; release, a later
no-button event, re-entry, blur, Escape, revision/document change, composition
or reaching the document edge stops it. No selection polling or frame loop is
active in ordinary reading.

The HTML held-pointer regression failed before implementation. Tests now cover
source-preserving frame advancement, head extension with a fixed anchor,
release cancellation, ordinary-text motion and cancellation, actual dispatch
outside the editor hitbox, and bounded edge velocities. **410 workspace tests
pass** (two existing installed-font tests ignored), with clean warning-denied
workspace Clippy and all 16 Python harness tests passing.

Fixture 45 and `--select-hold-seconds` exercise a stationary native drag across
offscreen content. The harness retains the selected-state screenshot even if
copy fails. Initial native failures exposed the shell-inset event gap; the
final pointer route addresses it rather than weakening the copy oracle.

The final normal release is
`3fc0ae7865e08e1604793c66d9e83a8e57859bd838e2e87bc63a9214c36169a2`.
At 1280×650, 100% text and compositor scale, a three-second stationary drag
from the first HTML fragment into the bottom shell inset copies every selected
marker and the offscreen second HTML fragment once in source order
([selection report](layout-previews/spec-drag-scroll-outside.selection.json)).
The complete edited Markdown is exactly `# Drag scroll selection`, `Before`,
and `Alpha x` with normal paragraph separators; one undo restores all original
source bytes ([edit report](layout-previews/spec-drag-scroll-outside.edit.json)).
The [selected native state](layout-previews/spec-drag-scroll-outside-selected.png)
was inspected for the ordinary and HTML selection highlights.
The same outside-pointer check failed on the previous release, and the
hover-only intermediate build also failed before the outside route was added.

Reproduce the drag using `--fixture 45-drag-scroll-selection.md --width 1280
--height 650 --select 313 231 360 630 --select-hold-seconds 3`. The complete copy
and edited-source check outcomes are recorded in the linked reports; the start is
inside `café`, and the endpoint reaches the end of `After` after scrolling.

An upward native drag from the last paragraph into the top inset also passes:
after scrolling to the end, `--select 304 616 260 50 --select-hold-seconds 3`
selects back through both HTML fragments to the title in exact source order
([report](layout-previews/spec-drag-scroll-upward.selection.json)). Replacing
that full range saves exactly `# x\n`, and one undo restores all original bytes
([edit report](layout-previews/spec-drag-scroll-upward.edit.json)). Its selected
state was visually inspected too. These checks use only copied synthetic
fixtures, private input/clipboard state and an isolated Weston display.

The same final release passes the isolated 10 MiB continuous-scroll gate:
**107.7 FPS, 8.14 ms draw p99**, with nonzero input-latency samples
(1728×1080, 166.7% compositor scale, 120 Hz, ten measured seconds;
[report](layout-previews/spec-drag-scroll-10m.json)). This measures ordinary
scrolling, not active drag-selection cost or the user's physical display.
Crusty validation reports 21 existing advisory findings and no new/worsened
findings. The interrupted final release build was confirmed terminal before
retrying; all verification processes completed.

### Cross-preview pointer integration and source-gap regression

The view now uses a lazy, immutable `PreviewSelectionProjection` when a drag
crosses an HTML preview boundary. It maps the source-order selection back to
canonical `PreviewPosition` endpoints for copy and authoring; the buffer is not
another editable document. Ordinary text-only drags retain the existing path.
HTML and intervening Markdown highlights, plain/rich copy, editing, reverse
selection and right-click position mapping use the same range. A reflow can
rebind the mapping only while its revision and text remain unchanged.

Two GPUI interaction tests exercise eight mixed-endpoint/direction combinations,
copy order, highlighting, one-transaction replacement/undo, zoom reflow and
source-preserving rejection of unsupported multi-block composition. This is not
a claim that multi-block IME is supported.

The native fixture-44 whole-file edit oracle exposed a serializer defect missed
by the earlier paragraph-only assertion: replacing `café → Middle → Beta`
produced the right text but eight redundant newline bytes before `After`.
The same assertion failed in document-core without the UI. Deleted source
units' whitespace-only prefixes were being retained, while generated blocks
also added a separator already present in the surviving source prefix.
`markdown.rs` now retains meaningful orphaned source (such as reference
definitions), drops deleted whitespace-only gaps, and inserts a generated
separator only after considering the next authored prefix. Surviving authored
spacing is not globally normalized. Regressions cover LF/CRLF, deletion through
the document tail, paragraph splitting next to authored extra spacing/reference
definitions, exact whole-file replacement, and byte-exact undo.

Release `0fc96fd8526a4595991ee0af8cfbd79e24f27d4f8429bc6d452e894c26026a83`
passes the isolated native fixture-44 checks at 1280×1100 and 100% text size:

- Forward and reverse native pointer drags copy exactly `café\nMiddle\nBeta`
  without changing source ([forward](layout-previews/spec-cross-preview-forward.selection.json),
  [reverse](layout-previews/spec-cross-preview-reverse.selection.json)).
- Typing `x` saves exactly the expected complete Markdown, preserving the italic
  suffix and normal paragraph boundary; one undo restores every original byte
  ([forward](layout-previews/spec-cross-preview-forward.edit.json),
  [reverse](layout-previews/spec-cross-preview-reverse.edit.json)).
- The [selected state](layout-previews/spec-cross-preview-selected.png) and
  [edited state](layout-previews/spec-cross-preview-forward-typed.png) were
  visually inspected. HTML and intervening Markdown show one contiguous logical
  range; the resulting paragraph retains its italic suffix.
- **406 workspace tests pass**, two existing installed-font tests remain ignored,
  and all 16 Python harness tests pass. Warning-denied workspace Clippy,
  formatting and diff checks pass. Crusty validation reports no new or worsened
  architecture findings (21 existing advisory findings).
- The same release passes the isolated 10 MiB continuous-scroll gate at
  **107.6 FPS, 6.34 ms draw p99**, with nonzero input-latency samples
  (1728×1080, 166.7% compositor scale, 120 Hz, ten measured seconds;
  [report](layout-previews/spec-cross-preview-scroll-10m.json)). This is not a
  physical-display measurement or a measurement of active drag-selection cost.

Cross-preview edge autoscroll is addressed above. Still incomplete: visual-line
Home/End and full cross-boundary keyboard navigation, multi-block IME, and the core's
existing nested cross-block editing restrictions. These remain required
authoring work; native pointer evidence does not close the full acceptance matrix.

### Canonical transaction foundation for cross-preview selections

`PreviewPosition` addresses either ordinary `DocumentPosition` text or a
source-verified HTML conversion leaf. `PreviewSelection` carries both endpoints
and the originating document revision. The resolver in
`document-core/src/document/preview.rs` validates the addresses, walks canonical
source order once, converts only selected preserved fragments on a private
working snapshot, and remaps both endpoints to the resulting canonical nodes.
It does not become another editable document or change layout preferences.

`DocumentSnapshot::preview_clipboard_payload` uses that private snapshot to
produce the existing plain/rich clipboard formats without changing the live
source, revision, selection or history. `EditCommand::EditPreviewSelection`
uses the same resolver before the normal replacement, paste, formatting, link
or split operation. Conversion plus authoring is one transaction; an error
never publishes a partially converted document. Stale revisions (including an
intervening undo), mismatched HTML, invalid leaves/UTF-8 offsets and unsupported
preserved content are rejected. Active composition still prevents mutation.

Seven regression tests cover:

- Forward/reverse HTML → Markdown → HTML selection, exact semantic/rich copy,
  replacement, one-step byte-exact undo/redo and unaffected sibling identities.
- A 48-case differential matrix against explicit conversions followed by normal
  edits: mixed endpoints, interior HTML, same-fragment endpoints, both directions
  and all six authoring operations.
- Distinct text leaves in one HTML fragment, without consuming its existing
  unselected caret paragraph.
- Failed and empty operations retaining the exact original snapshot and history;
  stale revisions after real editing/undo and active-composition rejection.
- Unselected HTML remaining shared and byte-preserved; selected opaque block
  structures rejected instead of flattened or lost.
- A plain-ASCII reverse-selection formatting/link regression uncovered by this
  matrix. The shared command range normalization now compares both node order
  and text offsets; selection direction is retained. This failed before the fix.

Workspace verification: **402 tests passed**, two installed-font tests ignored;
warning-denied workspace Clippy, formatting and diff checks pass.
Normal release `8bf3098c67eebb5748373589ae0b43f685b656f696f2081133867f7b30358931`
also passes the existing native single-fragment direct-edit check at 200%:
clicking preserves HTML, typing reaches the selected rich text while retaining
formatting/link content, and one undo restores the exact original HTML
([report](layout-previews/spec-preview-range-single-edit.conversion.json)).
This runs only on copied fixture 10 and an isolated native seat; it does not
claim native cross-fragment support yet.

This was the core foundation before the pointer integration documented above.
Existing normal editor restrictions on nested cross-block ranges and multi-block
IME remain; the new transaction does not bypass those restrictions.

### Display formula viewport containment

The fixture-43 denominator clipping is fixed in `editor.rs`. The generated SVG
already contained the complete glyph. A native GPUI layout regression instead
found a 12×48 image inside a 1888×46 scroll viewport: block layout enlarged the
image from its rounded intrinsic aspect ratio. An explicit flex row now honors
the measured image height while retaining local horizontal scrolling and
non-shrinking image width. No source, formula metrics or SVG paths were changed.

`display_fraction_image_fits_its_scroll_viewport` failed before the fix. It now
checks simple/nested fractions and summation limits at 80%, 100%, 150% and 200%,
asserting vertical containment, the original measured height (not shrink-to-fit)
and byte-exact Markdown. Temporary SVG print instrumentation was removed.

Normal release `4060e29e6bb69bc9a9f60c88d29e1f20ad8dae4be9dabc5fc56403fa6581a7f8`
has the following native evidence, using private Weston input and copied fixtures:

- [Fixture 43](layout-previews/spec-fraction-contained.png): the denominator's
  final two ink rows contain 9 and 10 dark pixels, compared with 1 and 2 in the
  clipped baseline. The same pixel oracle rejects the old capture and accepts
  the new one. Native typing inside `\frac{1}{2}` autosaves, and one undo restores
  every original source byte ([edit report](layout-previews/spec-fraction-contained.edit.json)).
- [200% capture](layout-previews/spec-fraction-contained-200.png): visually
  inspected full fraction and indexed radical. Native MathML hierarchy, token
  order, invalid-source fallback and unchanged source pass for all fixture-40
  equations ([AT-SPI report](layout-previews/spec-fraction-contained-200.math-atspi.json)).
- Workspace tests: **395 passed**, two existing installed-font tests ignored;
  warning-denied workspace Clippy, formatting and diff checks pass.
- The same release passes the isolated 10 MiB continuous-scroll gate at
  **109.6 FPS, 4.39 ms draw p99**, with recorded input-latency samples (1728×1080,
  166.7% compositor scale, 120 Hz, ten measured seconds after warm-up;
  [report](layout-previews/spec-fraction-contained-continuous-10m.json)). An initial
  [wheel-driver run](layout-previews/spec-fraction-contained-scroll-10m.json)
  recorded zero input-latency samples and correctly failed the gate; its high
  frame rate is not accepted as scrolling evidence. Neither run measures the
  user's physical display.
- The same isolated continuous-input configuration also passes the 80-display /
  80-inline [formula stress fixture](layout-fixtures/24-formula-stress.md) at
  **109.7 FPS, 4.46 ms draw p99** over ten measured seconds
  ([report](layout-previews/spec-fraction-contained-math-stress.json)). This
  measures scrolling performance, not visual correctness of every equation.

Reproduce the native edit check:

```sh
python3 performance/capture-layout.py --fixture 43-layout-recovery.md --width 1280 --height 1100 --startup-wait 8 --select 310 649 310 649 --edit-check --edit-within '\frac{1}{2}' --output /tmp/tachyon-fraction.png
python3 performance/capture-layout.py --fixture 40-accessible-math.md --width 1280 --height 1400 --startup-wait 8 --zoom-steps 10 --atspi-math-check --output /tmp/tachyon-fraction-200.png
```

This resolves this specific clipping defect, not the remaining full mathematical
visual, RTL, screen-reader speech or HTML cross-fragment editing acceptance.

### Native planner fault and delayed-result recovery

An explicit `layout-validation` Cargo feature now supplies one-shot native
fault actions. Normal builds do not compile their bindings, state, timers or
instrumentation. The feature is forwarded by `markdown-app` to `document-view`;
it adds no dependency. Its parent keyboard context includes Find, allowing the
harness to blur editing normally and settle the Find viewport **before**
requesting the fault through the existing diagnostic replan path. The hook does
not change source, selection, focus or published geometry itself. Opening a
different document clears a pending unconsumed fault.

The first native attempt deliberately reached a transient Find resize: the
failure report was marked stale (`viewport_zoom_or_recent_edit`) and discarded.
The harness was corrected to trigger after that transition; production commit
guards were not weakened. A second routing check confirmed that Find is outside
the document canvas's keyboard context, hence the validation-only parent scope.

Both final checks pass on validation binary
`6c9d39fd9c696244fde337c475e52b54919ba1377273c2b49fc750066c6ba177`, using only
private Weston/Wayland input, a private AT-SPI bus/clipboard and a copied fixture:

- [Planner panic report](layout-previews/spec-native-recovery-panic.recovery.json):
  a native measured 3×2 list becomes a non-overlapping source-order stack;
  all six native item identities remain stable, Markdown stays byte-exact,
  native copy has each item once in source order, a selected paragraph receives
  the exact edit, and one undo restores the entire source. The corresponding
  [planning trace](layout-previews/spec-native-recovery-panic.planning.json)
  includes a committed `planner_failed` report.
- [Timeout report](layout-previews/spec-native-recovery-timeout.recovery.json):
  a completed worker result is held beyond the real ten-second watchdog, with
  no replacement worker. Native AT-SPI bounds prove that the stack is visible
  **before** the held worker completes. The recorded sequence then confirms
  release and late-result discard; item bounds stay identical afterward.
  Source-order copy and exact edit/undo also pass.
- Inspected [before](layout-previews/spec-native-recovery-panic-before.png),
  [panic fallback](layout-previews/spec-native-recovery-panic-fallback.png) and
  [after late completion](layout-previews/spec-native-recovery-timeout-late.png)
  captures show the grid/stack transition, readable recovery message, retained
  HTML content and no blank document. These screenshots supplement rather than
  replace the source, identity, geometry and editing assertions.
- Feature-enabled workspace tests: **394 passed**, two existing installed-font
  checks ignored. Feature-enabled warning-denied Clippy passes. All **16**
  Python harness tests pass, including rejection of missing/duplicate items,
  empty geometry, overlap and a malformed second grid row.
- Default-feature workspace tests also pass **394 tests** (two ignored), with
  warning-denied Clippy, formatting and diff checks clean. The normal launcher
  was rebuilt without the feature as
  `e431c52088d37a58452cbd167dd3b9b42f409347d41bd23cfb13d0711ae3df9a`;
  executable inspection confirms no `TACHYON_LAYOUT_VALIDATION` instrumentation.
- The same normal release passes the isolated 10 MiB continuous-scroll gate at
  **109.8 FPS, 4.19 ms draw p99** (1728×1080, 166.7% compositor scale, 120 Hz,
  ten measured seconds after warm-up;
  [report](layout-previews/spec-native-recovery-normal-scroll-10m.json)). This
  is not a physical-display performance measurement.

Reproduce without leaving fault controls in the normal launcher:

```sh
cargo build --release -p markdown-app --bin tachyon --features layout-validation
validation_run=$(mktemp -d)
install -m755 target/release/tachyon "$validation_run/tachyon"
cargo build --release -p markdown-app --bin tachyon
python3 performance/capture-layout.py --binary "$validation_run/tachyon" --fixture 43-layout-recovery.md --width 1280 --height 1100 --startup-wait 12 --layout-trace summary --layout-recovery-check panic --output /tmp/tachyon-panic.png
python3 performance/capture-layout.py --binary "$validation_run/tachyon" --fixture 43-layout-recovery.md --width 1280 --height 1100 --startup-wait 12 --layout-trace summary --layout-recovery-check timeout --output /tmp/tachyon-timeout.png
```

This supplies native evidence for the reading-mode AT-15 fault cases; it is not
evidence for killing a blocked synchronous native call, recovery before a stack
is ready, or failure-time resize with an existing focused edit lock. The full
spec remains incomplete. The small `\\frac{1}{2}` in fixture
43 appears to lose the bottom of its denominator in both the pre-fault grid and
the recovered stack. The inspected
[normal-release capture](layout-previews/spec-recovery-fixture-normal.png) shows
the same symptom without validation controls. The containment fix and regression
above resolve that finding; these recovery checks alone are not complete
mathematical visual acceptance.

### Ready stack publication on optimizer timeout

Before optional adaptive planning, the reflow worker prepares a current-width,
source-order stack with the normal measured text/HTML/math renderer. A single
ready recovery slot is shared with its watchdog. The UI takes only completed
geometry and uses a nonblocking slot read; it does not render a fallback on the
UI thread or spawn a replacement optimizer while the old worker is still live.

Timeout and normal completion now share one guarded commit method. A timeout
can publish the ready stack, preserving the current reading anchor, original
source, selection and current resource/environment checks. It retains actual
worker ownership. Late completion cannot replace that stack or overwrite its
new geometry-generation retry key. The observer also uses this recovery path
if it notices absolute deadline expiry before the watchdog callback runs.

Each adaptive geometry version retains at most one baseline stack. Exact
projection/font/image/width/zoom matching reuses it across planning-window
transitions; a changed environment rebuilds it. Baselines contain no links to
older geometry generations. The additional retained baseline costs memory and
cold preparation, so release scrolling is checked separately below.

Evidence:

- The timeout regression was red with an old 1100-pixel grid still installed
  after a requested 360-pixel reflow timed out. It now verifies the fresh narrow
  stack **before** releasing the held worker, retained HTML/math, unchanged
  selection/source, blocked replacement dispatch, rejection of the late result,
  retry suppression and recovery after a subsequent width change.
- Intervening-composition tests cover worker failure, planner fallback and
  watchdog timeout. Ready recovery does not replace provisional geometry/text,
  marked range or selection, and Escape still leaves no undo entry.
- Retention tests verify exact-width reuse, invalidation on changed width,
  single-use recovery ownership, HTML/math availability, and absence of a
  baseline history chain.
- Workspace tests: **394 passed**, two existing installed-font tests ignored;
  warning-denied workspace Clippy, formatting and diff whitespace checks pass.
- Release `2d1804555bdc4f36ea7fc4a24cea6897333955da907bbaac23a4d9d0917d8d19`
  passes the isolated 10 MiB continuous-scroll gate: **109.2 FPS, 5.21 ms draw
  p99**, 1728×1080, 166.7% compositor scale, 120 Hz and ten measured seconds
  after warm-up ([report](layout-previews/spec-ready-stack-scroll-10m.json)).
  This is a normal-workflow budget check, not a physical-display measurement.
- The inspected [normal capture](layout-previews/spec-ready-stack-normal.png)
  retains peer rows, list grids and source order. Its
  [trace](layout-previews/spec-ready-stack-normal.planning.json) records four
  commits, one stale discard, no planner failures, and separate baseline
  preparation stages. Later focused replans do not prepare a new recovery stack.
- The same release passes native find, contained-overflow reveal, disclosure
  opening, exact clipboard text, HTML text-to-Markdown editing and exact undo at
  360×1400 with 200% text size
  ([report](layout-previews/spec-ready-stack-find-200.find.json),
  [inspected capture](layout-previews/spec-ready-stack-find-200.png)).

Scope remains explicit: no ready fallback is published if initial baseline
preparation itself fails/times out, or if a focused edit lock forbids the stack.
Those cases retain the previous view. Native fault injection and recovery-time
resize with an existing edit lock still need acceptance evidence. A stuck
synchronous native call is not forcibly killed. AT-15 is not yet declared fully
accepted.

### Adaptive-planner panic produces a fresh stack

The optional optimizer now has a narrower unwind boundary inside the owned
reflow worker. A planner panic discards partial candidates and builds the
semantic source-order plan without old arrangements, then runs the normal
measured geometry/Blitz/math renderer at the requested width. This differs from
the outer worker failure policy, which retains the previous published geometry.
The stack commits through the existing revision, environment, selection,
composition, and current-anchor guards. A stale fallback cannot publish status
or replace newer authoring state. An existing focused edit lock is not dismantled
by this recovery path; that case retains the prior view instead.

After a successful fallback commit, the shell reports the single-column
recovery. Retry suppression is keyed to the **newly installed** geometry
generation, preventing paint/scroll from immediately re-entering the failed
optimizer. Changed document/environment/focus inputs can retry normally. Trace
reports include `planner_failed`, independently of invalid-row validation.

Verification:

- The actual worker fault regression first failed: a requested narrow reflow
  retained wide geometry. It now exercises a measured grid at 1100 logical
  pixels, a planner panic during a 360-pixel resize, fresh stack geometry,
  source-order placement, all six items exactly once, retained rendered HTML
  and display math, unchanged selection/Markdown, no undo entry, 100 suppressed
  identical requests, and successful measured-grid recovery on widening.
- A preparation test verifies recovery diagnostics and refusal to dismantle an
  existing edit lock. The intervening-composition regression now covers both
  a failed worker and a successfully prepared fallback: neither can overwrite
  provisional text, selection, or marked range; Escape leaves no undo entry.
- Workspace tests: **393 passed**, two existing installed-font checks ignored.
  Warning-denied workspace Clippy, formatting and diff whitespace checks pass.
- Release `b8aff2d59ee49d651616d587ed69d4cbfa9d0eba9f4def4db8a27dd89934c8ff`
  was checked in the private Weston environment. The inspected
  [normal rendering capture](layout-previews/spec-planner-recovery-normal.png)
  retains the peer row, six-item grid, section order and spacing. Its
  [trace](layout-previews/spec-planner-recovery-normal.planning.json) has four
  committed reports, one stale discard, and no planner failures.
- The same release passes the isolated 10 MiB continuous-input scrolling gate:
  **109.5 FPS, 4.64 ms draw p99**, 1728×1080, 166.7% compositor scale, 120 Hz,
  ten measured seconds after warm-up
  ([report](layout-previews/spec-planner-recovery-scroll-10m.json)). This is a
  normal-workflow regression check, not native fault-injection or a physical
  display measurement.

This is progress on AT-15, **not full acceptance**. Native compositor fault
injection, a fresh stack on timeout/global rendering failure, and failure-time
resize with an active edit lock still need end-to-end recovery evidence. The
ten-second watchdog continues to retain actual worker ownership and discard
late results; this change does not claim to interrupt a stuck synchronous call.

### Timed-out reflows retain ownership until real completion

Reflow now has a ten-second recovery deadline (separate from scrolling's frame
budget and from initial file loading/preparation). An owned watchdog reports a
timeout and requests cooperative cancellation. It does **not** release the live
worker slot. Only actual worker completion can release that slot; any late
geometry is discarded, and the timed-out input remains suppressed until it
changes. The completion observer also checks elapsed time, so a delayed UI poll
cannot publish an expired result merely by running before the watchdog callback.
Normal completion drops/cancels the watchdog task.

The preparation pipeline checks cancellation before work and after projection,
table measurement, image binding, planning, retained geometry/extensions and
diagnostic collection. A currently running native shaping/Blitz/math call is
not forcibly interrupted. Once it returns, the next checkpoint prevents further
stages. Cancellation uses a monotonic shared flag, not publication of mutable
editor data. Panic and timeout outcomes remain distinct. Current source,
selection and geometry were retained by this original supervision change. The
ready-stack follow-up above now permits guarded timeout publication when a
current recovery baseline is available.

Evidence:

- `timed_out_reflow_keeps_ownership_and_discards_late_geometry` was red before
  connecting the watchdog (no timeout status after advancing the test clock).
  It now holds an actual dispatched worker result asynchronously, advances the
  GPUI clock past the deadline without sleeping, verifies status/live ownership/
  unchanged geometry and selection, proves a second request did not consume its
  dispatch hook, releases the old result and verifies it is discarded. Identical
  input stays suppressed; a new width succeeds. Advancing the clock again after
  success confirms that the cancelled watchdog does not produce a stale error.
- `reflow_cancellation_at_each_stage_preserves_canonical_content` forces expiry
  at each of seven checkpoints on a document containing a table, HTML fragment
  and formula. Each returns Timeout without changing source or history. Allowing
  all seven checks completes with retained HTML and rendered display math.
- Coordinator tests cover timeout/document-switch ownership, duplicate expiry,
  stale callbacks, late success and deadline/flag checks. Test-only counters and
  the held-result channel are absent from the release build. The channel uses
  the already-resolved workspace futures 0.3.34 as a **dev dependency**; no new
  package version or release dependency was introduced. Context7's futures
  guidance and pinned GPUI scheduler source informed the non-blocking harness.
- Workspace suite: **391 passed, two existing installed-font tests ignored**.
  Workspace all-target check, warning-denied Clippy and formatting pass.
- Release SHA-256 `bf286df723a0ddb96a12237e4054dcdc965bb205ac83373d4aab2342ae2f685d`:
  fixture 42 passes native find, local-overflow reveal, disclosure reveal,
  copy, HTML first-edit conversion and exact undo at **360×1400 / 200% text**,
  100% display scale. Visually inspected the final narrow capture. Artifacts:
  `layout-previews/spec-reflow-timeout-find-200.{png,find.json}`.
- Native fixture 21 at 1280×1000 / 100% text/display completes six measured
  commits (one stale result discarded), including the HTML-heavy preparation
  at 382.64 ms and two requested warm replans. Inspected the authored open/closed
  disclosure rendering and shared spacing. This normal-workflow capture does
  not force a timeout. Artifacts:
  `layout-previews/spec-reflow-timeout-disclosures.{png,planning.json}`.
  Both native runs use isolated fixture copies, state, clipboard, Wayland and
  session buses with 12-second startup allowance; fixture 42 additionally allows
  three seconds for the text-zoom reflow.
- Python harness: 13 tests pass. Crusty `task_592709ec5560a48c` completed with
  21 existing findings and no new/worsened findings.
- Same-release isolated 10 MiB continuous-scroll gate: **109.4 FPS, 3.92 ms
  draw p99**, ten measured seconds after 14-second warmup, 1728×1080 output,
  166.7% display scale and 120 Hz Weston. No concurrent build or capture during
  the gate. Report: `layout-previews/spec-reflow-timeout-scroll-10m.json`.
  This is one gate run, not a performance-improvement or physical-device claim.

Remaining AT-15 scope: native end-to-end timeout/failure injection and a verified
legal stack at every resized viewport are still incomplete. A call that never
returns keeps the single worker slot occupied: the UI keeps the current view,
but no replacement reflow is started until that call ends. This is intentional
bounded ownership, not forced thread cancellation or deadlock recovery. Initial
file loading/preparation, thread creation failure, abort and OOM are not covered
by this reflow deadline. Full-spec completion remains unproven.

### Supervised reflow failure and document-switch ownership

`editor/reflow.rs` now owns one active request ticket per editor. Switching
documents clears the old failure but does not pretend its synchronous worker
has stopped. A serial ticket prevents an old/duplicate completion from clearing
a newer request, even if its input key is identical. Failed input keys include
document and geometry generation, exact viewport/zoom, resource generation and
editing focus. Identical requests are suppressed until an input changes rather
than retried on every frame.

The dedicated worker catches unwinding preparation failures around its owned,
read-only snapshot work and returns an explicit failure outcome. Partially
prepared geometry is never installed. Completion releases the matching active
request and leaves existing geometry, selection and content untouched. The
application's existing status banner reports the failure and is notified on
recovery; a failed old document cannot publish status into newer content.
No fault-injection setting or command was added to the release: injection is
compiled only into the Rust test build.

Evidence:

- `document_switch_keeps_ownership_of_the_running_reflow` was red before the
  implementation: reset released an unfinished dispatched worker. It is now
  green and checks that the replacement source survives stale completion.
- `failed_reflow_retains_content_suppresses_retries_and_recovers_on_resize`
  exercises actual editor dispatch/completion with a test-only worker panic,
  retains the identical published line allocation and selection, proves exact
  source and no undo entry, rejects 100 identical retries, then successfully
  reflows after a width change and clears status. First-paint geometry is
  settled before injecting the fault, so a legitimate initial width change
  does not accidentally test a different request.
- `failed_reflow_does_not_overwrite_intervening_composition` starts a worker,
  begins provisional `仮` composition before completion, then verifies identical
  geometry, selection and marked range after failure. Escape restores exact
  `base` with no history entry. This is a programmatic composition regression,
  not a native CJK candidate-window test.
- Pure coordinator tests cover unwind outcome, duplicate completions, changed
  inputs, suppression and live ownership across document switches.
- Workspace suite: **387 passed, two existing installed-font tests ignored**.
  Workspace all-target check, warning-denied Clippy and formatting pass.
- Rebuilt release `4f899b6f0334de1cd815a7407690f3f7e8024a9dcbaddd45d53516364986a572`
  passes native fixture 14 typing/autosave, two-second focused idle and exact
  undo. The insertion is inside Beta's `second explanation`; Alpha/Gamma source
  markers remain intact, and the inspected idle capture retains all three peer
  columns. Artifacts: `layout-previews/spec-reflow-supervision-edit.edit.json`,
  its `-typed.png`/`-idle.png` captures and restored `.png`.
- On the same release, native fixture 40's private AT-SPI check passes MathML
  structure/token order and unchanged source. The inspected capture includes
  inline superscripts, fraction, indexed radical/scripts and matrix with exact
  source panels. Both native runs use 1280×1000 output, 100% text/display scale,
  12-second startup allowance and isolated fixture/state/clipboard/Wayland/bus.
  Artifacts: `layout-previews/spec-reflow-supervision-math.math-atspi.json` and
  `.png`. These are normal-workflow checks, not native worker-fault injection.
- Crusty validation `task_d564013d10912f8f` completed: 21 existing findings,
  no new/worsened findings.
- The same release passes the isolated 10 MiB continuous-scroll gate at
  **109.5 FPS, 4.33 ms draw p99** over 10 measured seconds after a 14-second
  warmup, 1728×1080 output, 166.7% display scale and 120 Hz Weston. No concurrent
  build/capture ran during this gate. Report:
  `layout-previews/spec-reflow-supervision-scroll-10m.json`. This single-run
  result is not a performance-improvement or physical-device claim.

At this checkpoint timeout handling was still missing; the follow-up above adds
supervised expiry and cooperative checkpoints, without forced cancellation.
Native end-to-end fault injection and legal-stack recovery at every resized
viewport remain unverified. The full specification, HTML/math editing/
accessibility/scale acceptance and earlier Auto-only versus §15 control conflict
remain in scope, not redefined by this recovery step.

### Reject malformed retained rows before native placement

The AT-15/AT-20 audit found that `RowCandidate::legal` previously checked track
geometry but not progressing group ranges, column ownership or finite cost.
The DP's forward search separately bounded group consumption; its previous-plan
hysteresis loop did not, so a malformed zero-length previous interval could
repeat without advancing. This is an internal invariant failure, not a
reproduction of the user's physical-device scrolling report.

Row legality now rejects empty/reversed intervals, missing/duplicate group IDs,
missing/overlapping/out-of-bounds column parts, nonfinite cost and invalid flow
counts. Previous-plan hysteresis also enforces the current window end. Requests
beyond the 40-unit search bound return no plan instead of panicking. Retained
offscreen/focused rows pass validation before use, and old root lookup is checked.
The final placement boundary verifies IDs against canonical units and checks
kind-specific parts (whole peer/table groups, the actual explanation/example
suffix, or each gallery unit's final image). Invalid partitions are rebuilt as
complete stacks from canonical units, never from the failed candidate data.
Opt-in diagnostics report `invalid_row_windows`.

Verification so far:

- `cargo test -p document-view malformed_rows_are_rejected -- --nocapture`
  failed before the implementation on `non-progressing groups`, then passed.
  It checks ten malformed candidate mutations and source-order stack selection.
- `invalid_retained_row_falls_back_to_complete_rendered_source_order` injects
  a wrong canonical owner into an otherwise geometrically legal measured peer
  row. The focused-row path records one fallback; actual line construction
  produces every paragraph once, non-overlapping and in source order, with
  byte-identical Markdown. This is a renderer integration test, not native
  end-to-end fault injection.
- The existing 729 generated row-sequence cases now assert complete row legality
  as well as deterministic exact partitioning.
- Workspace tests: **381 passed, two existing installed-font tests ignored**.
  Workspace all-target check, warning-denied Clippy and formatting pass.
- Rebuilt release SHA-256
  `7a09ed9bfc0fb5875f6fa47f4b1d15391daf889a8cc99486d44ad1e61404a55f`:
  native isolated fixture 14 at 1280×1000 and fixture 12 at 1280×1200,
  both 100% text/display scale with 12-second cold-start allowance. Visually
  inspected three peer cards, the row-major unordered grid, unequal table pair,
  explanation/table pair and source-following paragraphs. Select-all/copy keeps
  the three peer paragraphs and closing marker once in source order.
  Both traces report zero invalid-row fallbacks for valid input; viewport/geometry
  stale results are discarded and later measured results commit. These ordinary
  native captures do not inject a worker failure.
  Artifacts: `layout-previews/spec-row-validation-peer.{png,planning.json,copy.json}`
  and `layout-previews/spec-row-validation-tables.{png,planning.json}`.
- Python harness: 13 tests pass. Crusty validation `task_3918567680507525`
  completed with 21 existing findings and no new/worsened findings.
- The same release passes the isolated 10 MiB continuous-scroll gate:
  **104.3 FPS, 14.06 ms draw p99**, 10 measured seconds after 14-second warmup,
  1728×1080 output, 166.7% display scale and 120 Hz Weston. No concurrent build
  or capture ran during this measurement. Report:
  `layout-previews/spec-row-validation-scroll-10m.json`. This is one gate run,
  not a performance-improvement claim or a physical-device measurement.

At this checkpoint worker error/timeout supervision was missing; the follow-up
above adds an unwind outcome and request ownership. Full native timeout/error/
stale-result acceptance remains incomplete, as does the full specification.

### Initial lists wait for measured geometry

The ordinary open/reload paths already run `PreparedDocumentView::prepare` in
background work; inspection did not justify claiming that file-open extension
rendering was on the UI thread. It did reveal an initial-layout violation:
`AdaptivePlan::build` could publish list grids from character-count estimates
before `measure_lists` had obtained native item geometry. Offscreen measured
planning already rejected speculative grids, but first preparation bypassed
that safeguard.

The initial builder now selects only List, Steps, Checklist or Outline from
content structure. Character-count height estimation and its separate grid
scorer were removed. New two/three-column grids come from the existing measured
candidate chooser. An already-published grid may still be retained through
the existing edit-lock path; this change does not dissolve a grid while typing
or introduce new user-facing layout controls.

`initial_view_stacks_lists_until_native_measurement` failed on the initial
guessed grid before the change. It now verifies source-order vertical geometry,
an unchanged stack if the user focuses the list before first measurement, and
a three-column candidate with native fit evidence when measured without that
editing lock. Semantic checklist/outline/step treatment remains covered at
different widths. Older renderer tests that initialized heuristic grids now
obtain real measured plans, retaining their original byte coverage, row-major
order, row non-overlap, arrow-stage order, caret and undo assertions. Native
measurement tests also verify 1280→696 px grid resize and focused live line
breaks on one edited canonical document, rather than fabricating a second
document to stand in for an edit. All 379 workspace tests pass (two installed-
font tests ignored), together with all-target check, warning-denied Clippy,
formatting and diff checks.

Release `d0b7addc2aa7c0fd6a1194e8f433e5b5232460110fe69f44508084f8f348ce07`
passes native fixture-02 checks at 1440×1000/100% text and 360×1400/200% text.
The narrow final screenshot is RGB-identical to the preceding release; the
wide image differs at one rasterized pixel (317,556), with the same visible
layout. Artifacts: `spec-initial-stack-{before,final}-{wide,narrow-200}`.
Native typing in the first measured grid card, a two-second focused idle,
autosave and full byte-exact undo pass in `spec-initial-stack-final-grid-edit.edit.json`.
The second and third grid columns remain RGB-identical in both typed and idle
captures. The same release passes the isolated 10 MiB continuous-scroll gate
at 1728×1080, 166.7% display scale and 120 Hz: **108.6 FPS, 7.34 ms draw p99**
over ten measured seconds, without a concurrent build/capture
(`spec-initial-stack-final-scroll-10m.json`). This remains an isolated-output
result, not physical-device verification.

The first native edit invocation used raw punctuation-bearing sentences as
byte-preservation markers inside the rewritten list and failed. Inspection
confirmed correct caret placement (`main` → `mxain`) and the existing
`escape_inline` policy: rewritten text escapes ASCII punctuation, including
apostrophes and periods. No serializer change was made to force a pass. The
accepted check verifies the exact insertion within a punctuation-free source
span and protects unique neighboring word spans; complete neighboring-card
pixel comparisons and byte-exact full undo provide independent fidelity checks.
The harness now includes counts and a bounded source excerpt in failed protected-
fragment assertions, rather than reporting an unexplained marker failure.

This is a correctness fix for the initial stack, not a startup-speed result.
Initial HTML/math preparation and document-wide metadata publication still need
the bounded preparation work described below. The original full contract and
remaining authoring/AT/IME acceptance work are unchanged.

### Retained HTML/math across planning-window transitions

`PublishedGeometry` now retains the source-local extension lines of its own
generation, before placement, group gaps and zoom. A worker may borrow this map
from the previous published generation and transfer matching immutable previews
into its new map. It does not retain previous maps, create a document-history
cache, duplicate raster payloads, or drop offscreen semantic/editing metadata.
It adds per-extension keys and source-local line wrappers proportional to the
published extension count. Oversized keys/line sets and unsupported HTML keep
the ordinary renderer path; this is not a new unbounded historical cache.

Keys use the existing weak canonical-leaf identity, local source range, exact
widths, container/table context and insets, font override, source-bound HTML
disclosures and loaded-image blob identities, document-start role and breaks.
A different measurement-provider identity invalidates reuse. Valid HTML returns
complete Blitz geometry before text measurement is consulted, so entering a
measured window can reuse that same preview. Display math retains editable
source lines too, and therefore distinguishes measured and estimated mode.
Placement, group spacing, paint order, component bounds and zoom are rebuilt
normally; no source-order or layout-policy changes were introduced.

The three-chapter HTML/math regression failed first with zero retained hits,
then with four rather than five hits when HTML unnecessarily depended on text
measurement mode. It now retains all three HTML previews and the two unchanged
math segments, while freshly preparing the newly measured math source lines.
It verifies fresh-build geometry/order/height parity, all extension presence,
unchanged source and shared HTML preview identities. Additional tests verify
UTF-8 source-range rebasing after a preceding text edit and one-undo restoration,
same-size image pixel replacement, resource removal and fallback. The existing
14-input invalidation matrix now also compares partial reuse with fresh builds,
including HTML pixels, accessible text, text ranges, disclosures, anchors and
hit geometry. All **379 workspace tests** pass (two installed-font tests ignored),
with all-target check, warning-denied Clippy, formatting and diff checks. The
13 Python harness tests also pass.

Performance contract: release builds on AMD Ryzen AI MAX+ PRO 395 / Radeon 8060S,
32 logical CPUs, Linux 7.2.3, Weston 14.0.2 private headless GL compositor,
1280×1000 output, 100% display/text scale, bundled fonts, fixture 21's 64 HTML
disclosures. Three before runs and three final runs use the same command:

```sh
python3 performance/capture-layout.py --fixture 21-disclosure-stress.md \
  --width 1280 --height 1000 --startup-wait 12 --layout-trace summary \
  --scroll 1000 --scroll-steps 80 --output <run-path>.png
```

The hypothesis was that unchanged extensions, not window-scoped text shaping,
dominated transition preparation. The falsifier was unchanged extension calls
or worker time within the baseline variation. Every final transition instead
records **64 retained requests / 64 hits**. Committed worker reports after the
first five initialization/settling reports show:

| Run | Before samples / median / sampled p95 | Final samples / median / sampled p95 |
| --- | --- | --- |
| 1 | 15 / 223.833 / 319.815 ms | 29 / 0.908 / 1.316 ms |
| 2 | 16 / 209.710 / 312.819 ms | 29 / 1.071 / 1.416 ms |
| 3 | 16 / 213.977 / 299.910 ms | 29 / 0.866 / 1.428 ms |

Percentiles use inclusive interpolation. The same scroll input produces more
completed window transitions with the faster worker; these are end-to-end
workload distributions, not paired identical individual jobs. Before runs
preceded final runs rather than randomized alternation. Cold reflow still takes
339–419 ms in the final runs and is explicitly excluded from these warm-window
numbers. This fixture meets the proposed cached-window budget; it does not prove
the budget for every document, font/resource state or platform.

Before binary: `ca7d09f2d3f1994c6a60a7c3485e951f09aefbbf515a17fd44746defd7806dad`.
Final binary: `610aac0255e6934f1e8b36347b34bf229e689d0e726b5cb3c2cfbedb0d56e95e`.
Raw reports are `layout-previews/spec-extension-transition-before{,-2,-3}.planning.json`
and `layout-previews/spec-extension-transition-final-{1,2,3}.planning.json`.
The reviewed final-3 scrolled screenshot has exactly equal RGB pixels to the
before screenshot. Intermediate `after-{1,2,3}` reports belong to binary
`01c3d38eda4bab3bb3293dfc39a5b2665e7ac0461f31da33a6504e667674d3ce`, which still
rebuilt the newly visited HTML preview; they are not the final result.

The final binary also passes private native HTML visible/hidden text,
disclosure state/identity and unchanged-source checks (fixture 28), structured
math/token-order/source checks (fixture 40), and find/copy/HTML first-edit/
byte-exact one-undo restoration at 360×1400 and 200% text (fixture 42).
Artifacts are `spec-extension-retention-final-html.atspi.json`,
`spec-extension-retention-final-math.math-atspi.json` and
`spec-extension-retention-final-find-narrow-200.find.json`. The same binary
passes the isolated 10 MiB continuous-scroll gate at 1728×1080, 166.7% display
scale and 120 Hz: **108.5 FPS, 7.14 ms draw p99** over a 10-second measured run,
with no concurrent build or capture (`spec-extension-retention-final-scroll-10m.json`).
This is a gate result, not a claimed improvement in frame time or verification
of the user's physical input device.

Remaining §17 work is not removed by this optimization: initial extension
preparation still visits the document, and projection/metadata/position-index
publication still includes document-wide work. A fully bounded initial stack
and lazy extension geometry require independent offscreen semantic metadata and
coordinated deferred find/anchor reveal, not silently omitting previews. Full
resource/font completion, broad measured-window budgets and the remaining
authoring/AT/IME acceptance matrix remain open under the original contract.

### Find reveal inside local overflow

`reveal_find_horizontal` resolves the exact canonical target line even when its
table column is outside the painted region. It uses existing font shaping,
alignment, column geometry and local scroll ownership. It scrolls only enough
to reveal a fitting match, leaves an already visible match alone, and retains
the logical beginning of a match wider than the viewport. The document's
horizontal offset is never changed. Verified HTML uses retained preview caret
geometry; no HTML parsing or shaping was added to the wheel/scroll path.

The exact-cell regression failed before this change (`find must paint the
clipped target column`) and passes afterwards. Its explicit-width table uses
valid `Dotted` metadata and checks real overflow before searching. An initial
assertion incorrectly used a helper that falls back to the last painted line;
the accepted regression requires exact range ownership and in-viewport text.
The wide HTML regression at 200% text separately failed because native caret
bounds omitted local scrolling; `html_caret_bounds` now subtracts the same
overflow-owner offset as painting. Source bytes and content undo are preserved.

The first native fixture-42 run also exposed a delayed formatting toolbar over
the find input. A clock-controlled regression reproduced that defect. Find now
cancels the pending selection-settle task and its animation generation rather
than merely clearing the visible flag. Eleven focused find tests and all 376
workspace tests pass (two installed-font tests ignored), with warning-denied
Clippy. Thirteen Python harness tests pass, including bounded whole-tree retry
for the exact stale AT-SPI accessible-object error; unrelated D-Bus errors are
not retried or suppressed.

Release `ca7d09f2d3f1994c6a60a7c3485e951f09aefbbf515a17fd44746defd7806dad`
passes native fixture-42 find/copy/edit/undo checks at 1280×1000/100% text and
360×1400/200% text. `spec-find-overflow-final-{wide,narrow-200}.find.json`
records exact copying of right and left Markdown table targets, the long-code
target and the wide HTML target, plus HTML first edit and byte-exact one-undo
restoration. The corresponding table/code/HTML captures were inspected: local
content moves while the page retains its leading edge; the delayed toolbar is
absent. At narrow/200% sizes the code/HTML matches exceed the available width,
so their logical beginning is visible with their remainder available through
local scrolling; this is not a claim that overwide matches fit all at once.

The `spec-find-overflow-{wide,narrow-200}` captures retain the intermediate
release before the toolbar correction. Both sets use the private native
Weston/Wayland harness, 100% display scale and bundled fonts; the sidebar is
visible only at wide width. No physical desktop or user Markdown was changed.
Crusty validation against `ctx_1644af69b624` has no new/worsened findings.
The final release passes the isolated 10 MiB continuous-scroll gate at
1728×1080 output pixels, 166.7% display scale and 120Hz: **109.7 FPS,
4.04ms draw p99** (`spec-find-overflow-scroll-10m.json`). This run has find
closed and no concurrent build/capture; it verifies normal scrolling, not
active search indexing latency or physical-display performance.
Oversized unsupported HTML still uses the existing source fallback; this work
does not extend raster limits or promise exact opaque-fragment hits.

### Canonical find, HTML disclosure reveal, and edit safety

`editor/search.rs` implements Ctrl+F, forward/backward source-order navigation,
wraparound, result counts and Escape back to the editor. The fixed find bar is
outside the document scroll root. Indexing reads immutable canonical snapshots
on one background worker per view; generation/serial checks reject stale results.
Only the requested result is retained, not a vector of all occurrences. An
open search refreshes after content changes without selecting a result over the
user's new caret. Finding itself does not change source or content undo.

Supported HTML matches use the converter's exact text-leaf identities. A
source-bound DOM walk identifies only their containing disclosures; opening
those disclosures requests a reflow before placing the verified text selection.
Opaque matches have an explicit read-only copy payload so typing cannot mutate
an unrelated canonical caret. The regression uses a spanned HTML table: an
inline custom tag was an invalid fixture for this case because Markdown may
parse it as an ordinary paragraph. The revised test asserts preserved HTML and
absence of an editable target before checking copy and rejection of typing.

Seven focused Rust tests pass: canonical order and Unicode offsets, repeated
disclosure text, reveal after a prior selection, cancellation on close, opaque
copy/edit safety, content-refresh caret stability, and latest-query publication.
The complete workspace passes 372 tests, with two installed-font tests ignored;
all-target checking and warning-denied Clippy pass. The native helper now
checks the probe's actual `button` role, unique accessible names and click
actions; three Python regressions reject missing/duplicate/inactionable controls.

Release `cbc32948b229e6b36a523f84c0bbf9cdf336b998934f30bf58954e789ad44fc6`
passes the complete native find workflow at 1280×1000/100% text and
360×1400/200% text: named actionable buttons, source-order navigation and wrap,
exact clipboard text, opening both containing HTML disclosures, editing the
selected HTML words, exact one-undo restoration, and no-result handling.
Reports/captures: `spec-find-final.find.json`, `spec-find-narrow-200.find.json`
and their `-html.png` images, both visually inspected. Environment is private
Weston/Wayland, 100% display scale, bundled Fraunces/Spline fonts; sidebar is
visible at 1280 and hidden at 360. No physical desktop or user source is edited.
The older release `d26f7153…` exposed unnamed icon buttons; it predates the
current `NamedButton` wrapper and is not the passing release.

The first two narrow probes failed because AT-SPI child lookup returned None
while disclosure reflow changed the tree. `stable_traversal` now retries the
entire capture up to four times, clearing all partial records each time; it
never silently skips a vanished node or suppresses other errors. Two regression
tests verify clean retries, the retry bound and propagation of other failures.
All 12 Python harness tests pass. The production reveal logic was not weakened
to accommodate the probe. Crusty validation against `ctx_4ae85fab5c83` reports
21 existing advisories with no new or worsened findings.

The same release passes the isolated 10 MiB continuous-scroll check at
1728×1080 output pixels, 166.7% display scale and 120Hz: **109.1 FPS,
6.05ms draw p99** (`spec-find-scroll-10m.json`). The find bar is closed in
this performance run; this verifies the modified editor root's ordinary
scrolling, not active index-building latency or the spec's separate planner
budgets. No concurrent release build or capture ran during measurement.

Remaining find limitations: opaque-fragment matches reveal the fragment rather
than exact inner text geometry; full Unicode casefold/normalization is not
implemented (matching uses Unicode lowercase). Local overflow reveal is now
implemented as described above, with native verification recorded separately.
Only the active result is highlighted. These limitations and the rest of the
acceptance matrix keep §13 and WP-07 incomplete.

### Native math tokens: reproduced empty values and corrected publication

The first native fixture-40 check against release `9b2bf4f0…` failed with
`<math><mfrac><mrow><mn /></mrow><mrow><mn /></mrow></mfrac></math>`.
The semantic element hierarchy reached AT-SPI, but the numerator and denominator
were empty. This was a publication defect, not a formula-layout or parser error:
AccessKit 0.24.1 requires `Role::Label` text in `value`; the application supplied
`label`. The pinned adapter correctly derives those nodes' names from `value`.

`math_markup_node` now uses that native text contract. A regression walks the
real prepared fixture equations through the production node constructor and
checks text values, tags, invalid-formula fallback and exact source preservation.
It failed with `None` versus `Some("37")` before the correction and passes
afterwards. No adapter behavior, formula geometry, source editing or layout
choice changed in this correction. Workspace checks pass 365 tests (two
installed-font tests ignored), two separate adapter tests, seven Python harness
tests, all-target checking and warning-denied Clippy.

Release `51272e3cff3020208e6896d5a12be65e00e47bfeb7b949d456c240cbee207c8b`
passes the actual private-bus AT-SPI reconstruction at 768×1200 and 360×1400
with 200% text (`spec-math-values-{native,narrow-200}.math-atspi.json`). All
four equations retain exact source labels and ordered tokens; matrix rows/cells
and radical/script order are checked, with source-only fallback for the invalid
formula. Both screenshots were inspected. The narrow first viewport contains
the title/intro and start of the fraction panel; the native check still covers
all offscreen equations. It is not evidence that every formula fits that first
viewport, nor an audible Orca/MathCAT or braille-device test.

MathML comes from the same parsed AST retained with display geometry, with
bounded traversal and source-only fallback for unsupported constructs. The
native tree preserves MathML tags through the existing vendored adapter and
uses stable child identities. No math parsing or MathML compilation is added
to scrolling. Inline math still retains its paragraph semantics rather than
this structured display-math subtree; broader RTL and full text/selection
accessibility remain open.

The same release passes the isolated formula-24 continuous-scroll gate with
80 distinct display and 80 inline formulas: **109.7 FPS, 5.67ms draw p99**,
829 input-latency samples, 2,467 published AT-SPI nodes, and zero reported
25ms application stalls (`spec-math-values-at-scroll.json`). Environment:
1728×1080 output pixels, 166.7% display scale, 120Hz private Weston/Wayland,
24s configured warmup and 10s measured input, accessibility active. This
establishes this formula workload's scrolling gate, not physical-device
momentum behavior or the spec's separate cached/measured planning budgets.

The native formula-source edit check also passes with AT-SPI active: clicking
the numeral at (110,405) in the 768×1200 fixture inserts `x` inside `37`, saves
the edit, holds focus for two seconds and restores every original byte with one
undo (`spec-math-values-edit.edit.json`). The typed screenshot shows `3x7` in
both the actual fraction and its editable LaTeX. Earlier coordinate trials at
(122,405) and (102,405) inserted beside the brace and before the numeral,
respectively; neither was counted as a passed targeted edit. The source oracle
was not relaxed.

At the math-token audit baseline, §13 had no find-in-document action. That
historical gap is now partially addressed by the canonical find implementation
above; source-only anchor reveal remains separate from search and the existing
heading/HTML-anchor navigation.

### HTML table spans, cell spacing, and bounded overflow

Fixture 39 and a red core regression exposed loss of `colspan`/`rowspan` during
HTML import: the second-row continuation moved under the wrong column. The
typed Markdown table adapter now rejects nontrivial/uncertain spans, preserving
the complete original HTML for Blitz. Explicit conversion and direct text-edit
targets are not offered where conversion would flatten those relationships.
Core checks cover exact fragment bytes across neighboring edits, save/reopen,
and undo, plus safe alignment-attribute retention without event handlers.

The preview uses 10px vertical / 12px horizontal cell padding, theme-derived
header fill and 1px rules. Separate borders with one shared edge per cell avoid
Blitz's collapsed-grid rule crossing a row-spanning cell. Alignment is checked
using actual shaped text coordinates, including uppercase attribute values and
authored CSS overriding presentational hints. No font shrinking is used.

The first padded narrow nested-table capture fell back to plain text. Preview
preparation now negotiates measured table width in at most three attempts,
bounded by 1920 logical pixels and the existing raster/source budgets. The
top-level HTML surface uses the editor's existing horizontal-scroll ownership;
image, links, disclosure controls and selection move together under one native
clip, while HTML actions stay at the viewport's leading edge. Scroll handlers
reuse retained pixels and geometry; they do not invoke Blitz. Nested previews
inside canonical table cells retain their existing whole-table scroll contract
and use source fallback if the fragment cannot fit that cell.

Focused tests verify complete nested-table text at a 296px requested width and
200%-text horizontal scroll limits, adjusted hit coordinates, and unchanged
Markdown bytes. The workspace passes 362 tests (two installed-font tests remain
ignored), all-target checking, and warning-denied Clippy.

Release `8c4c5e70…` passes native source-order copy of all 18 fixture-39 markers
at 768×1200 (`spec-html-spans-final.copy.json`). Visually inspected captures
show correct merged borders at 100% and 200% text (`spec-html-spans-final.png`,
`spec-html-spans-200.png`, the latter at 1280×1400). At 360×1400, the nested
table's left and right captures show both ends of the actual table reachable
without page overflow (`spec-html-nested-overflow-{left,right}.png`).
The native fixture-38 first-edit check pastes eight bytes, waits two focused
seconds, zooms to 120%, preserves unrelated fragments and restores the exact
original source with one undo (`spec-html-padding-edit.edit.json`).

The fixture-36 narrow toolbar check failed at its old hardcoded point (80,1005),
including after restoring the cell's original subtree. This does not establish
a clipping regression as the cause. At the verified trigger point (65,995),
release `a488554a…` passes both exact clipboard commands, native keyboard menu
activation, conversion preserving the neighboring disclosure, and exact undo
(`spec-html-padding-cell-toolbar-hit.toolbar.json`, menu screenshot inspected).
Canonical cells retain their existing shared clip; only standalone HTML receives
the new local overflow viewport. The harness coordinate remains an explicit
argument rather than weakening any clipboard/source assertion.

The final release `a488554a63c478ac80891eb8d7b84d7d3c15908e42740db136892572c601bc20`
passes the isolated fixture-17 continuous-scroll gate at 1728×1080,
166.7% display scale / 120Hz, with 12s startup and 10s measured input:
**109.5 FPS, 3.07ms draw p99** (`spec-html-table-overflow-scroll.json`). This is
not physical-device momentum verification or proof of the 8ms/50ms planning
budgets. Five Python harness unit tests pass with `unittest discover -s
performance`; the earlier root-module invocation failed due to its import path,
not an app regression. Final formatting/diff checks pass. Crusty validation
`task_437d8e9d2852e5cb` completed with 21 existing advisories and no new/worsened
findings against `ctx_03b8b8b784d6`.

This is not complete HTML/AT acceptance: spanning tables remain opaque to
rich-text conversion, `rowspan=0` and exotic authored table CSS require further
renderer fidelity checks, and overflow beyond the bounded renderer uses the
complete source-order fallback. Full goal and WP-01–07 scope remain open.

### Narrow HTML actions and complete table-fragment ownership

The HTML toolbar now uses a native overflow menu when its actual on-screen
width is below 220px, retaining the 32-document-pixel header geometry. The
trigger keeps a visible/native accessible HTML label, drops the decorative
ellipsis at very small widths, and exposes the same Copy original HTML, Copy
fragment text and eligible Edit text commands. Wide previews retain their direct
buttons. Context7 menu documentation was checked against the pinned component
source; no dependency revision changed. The native-desktop skill's running-app
check verifies that the menu escapes cell clipping but stays inside the window.

The new private-seat `--html-toolbar-check` on fixture 36 verifies exact source
and text clipboard payloads (with a sentinel to prevent stale-clipboard passes),
keyboard menu-item activation, conversion with strong text and neighboring
disclosure source retained, and one-step exact undo. Release `72d5a943…` passes
this check at 360×1400 (`spec-html-toolbar-narrow.toolbar.json`); its menu
screenshot was visually inspected. No preview/body/caret coordinate offset was
changed to obtain the compact toolbar.

Fixture 37 exposed actual nested-table content loss: the old native view showed
only "Unsupported or preserved HTML" where the inner cells belonged
(`spec-nested-table-before.png`). Import now retains a complete authored nested
table as an opaque HTML fragment for the existing inert Blitz renderer instead
of constructing incomplete typed cells. Lossy rich-text conversion is not
offered for nested table structure. Canonical nested-table export also serializes
the actual nested table recursively, rather than writing a placeholder. Both
new core regressions failed before these changes and pass afterward. They cover
all exported cells/alignment attributes, exact fragment source through edits and
reopen, and original-source undo.

Release `72d5a943…` was visually inspected at 768×1000 and 360×1000: all inner
headers/values and the outer neighbor render in source order
(`spec-nested-table-{after,narrow}.png`). Native select-all/copy finds nine unique
markers exactly once and in order (`spec-nested-table-after.copy.json`). This is
the spec's complete opaque fallback, not a claim of nested typed-cell editing.

Further boundary tests found that `html_table_data` searched for the first
descendant table anywhere in a fragment. It therefore omitted surrounding prose
and second tables. Fixture 38's before capture demonstrates all three omissions.
The typed adapter now requires the body to contain one complete standalone
table, with no meaningful siblings or authored head elements. Wrappers, mixed
fragments and adjacent tables retain their complete HTML; explicit supported
conversion still retains before/table/after text. The new ownership regression
failed before the fix and now passes.

Current release `f7b50458ebb43eae5aa719aee97319bdea11a7f66d3e9e710be3cb281938fcad`
passes fixture 38's native source-order copy check: 19 unique markers, including
all formerly omitted prose and the second table, occur exactly once and in order
(`spec-html-boundaries-after.copy.json`). The 768×1400 native screenshot was
visually inspected against `spec-html-boundaries-before.png`. Direct first-edit
paste into the rendered wrapper prose, eight inserted bytes, two-second focused
idle, 120% zoom and exact one-step undo pass
(`spec-html-boundaries-edit-verified.edit.json`). Its active-edit screenshot
retains the caret, converted table and following prose, and untouched subsequent
HTML fragments. The first attempted native check incorrectly required a converted
paragraph's period to retain its original source spelling; canonical Markdown
escapes punctuation. The corrected check validates the converted text span and
still requires byte-exact untouched fragments and complete original-source undo.
All 356 Rust tests pass (86 core, 230 view, 1 fixture, 39 app), two installed-font
tests ignored. Workspace all-target check, warning-denied Clippy, formatting,
whitespace checks and five Python tests pass. Crusty reports no new/worsened
findings against the 21 baseline advisories.

The current release was also visually inspected at 1280×1400 with 200% document
text (`spec-html-controls-200.png`): the direct HTML actions fit their available
on-screen cell width, authored open/closed states remain visible and the following
paragraph remains reachable. The toolbar intentionally retains native UI text
size rather than scaling its labels with document typography.
The same release passes the isolated fixture-17 continuous-scroll gate at
**109.4 FPS / 3.51ms draw p99** (`spec-html-ownership-scroll.json`; ten measured
seconds, 1728×1080, 166.7% display scale, 120Hz, 12-second startup wait). This is
not a physical-display measurement or completion of the separate planning-budget
contract. All native interactions remain confined to private harness state,
synthetic copies and the isolated Weston seat.

### Rich cell panels, first-edit stability, and save/reopen fidelity

Fixture 36 adds multiline and long unwrapped code, open/closed authored HTML
disclosures, and independent neighboring cells. The initial native capture
`spec-rich-panels-before.png` showed code headers outside their rows and
disclosures flattened during table import. Code cells now reserve their header
and panel padding in row height; measured constraints include full unwrapped
code lines. HTML preview geometry retains its table/cell ownership and local
insets. Cell-local native overlays and painted code panels share the table's
horizontal offset and clipping boundary. Enclosing alert headers remain outside
that local clip.

Table import preserves disclosure/styled block fragments until explicit editing.
The canonical HTML conversion transaction can replace a nested fragment in its
own block sequence while retaining enclosing table/list IDs and untouched sibling
allocations. The core first-edit regression covers retained table membership,
inline strong text, same-cell selection, one-step exact undo, and provisional
composition cancellation. Geometry regressions cover code header/padding and
authored HTML disclosure state inside rows.

Native review required by the native-desktop skill found a separate active-edit
failure: `spec-rich-panels-direct-edit-typed.png` showed the first column growing
from roughly 335px to 655px on the first HTML keystroke. The new actual-editor
regression reproduced column fractions changing from 0.3573 to 0.6743. Structural
projection refresh discarded the focus lock. It now transfers the existing table
lock only when the caret remains in that canonical table and the column
definitions are unchanged. Tests also verify that leaving the table or explicitly
changing columns releases the lock. Focused reflow retains the same geometry.

A stronger native `--edit-preserve` check then exposed data loss not visible in
the running model: autosave serialized the untouched closed disclosure as
`<p data-preserved-source="true">` containing only its description. HTML table
serialization now retains `PreservedSource.source`, not its display description.
The renderer's inert/sanitized policy is unchanged. A subsequent reopen regression
also caught indentation before a preserved container being parsed as an indented
code block. Table import now recognizes these preserved containers as block
children when ignoring inter-block formatting whitespace. The core test verifies
the untouched disclosure remains a `PreservedSource` with the exact HTML after
save and reimport, as well as exact original-source undo.

Intermediate release `e67fb5ff…` passed native disclosure mouse/Enter/Space and
Tab/Shift+Tab activation, exact restored body geometry and unchanged authored
source (`spec-rich-panels-disclosure.disclosure.json`). Its first-edit/idle/120%
zoom/undo check passed, but the subsequently discovered column movement and
save/reopen loss mean that earlier check alone is not a fidelity pass.

Release `e26e10222087ffe8ce2a75f8ef14daf9a0411f947d7c4f6c556b774f1d3d2119`
passes the strengthened native first-edit test: seven-byte paste into the rendered
HTML body, four byte-exact neighboring source fragments (including the entire
closed disclosure), two-second focused idle and one-step exact undo
(`spec-rich-panels-final-edit.edit.json`). The typed screenshot was visually
inspected and retains the original first-column edge while exposing the edited
text and caret. The source-preservation check failed on release `b570ece7…`
before the serialization/import fixes. Current Rust verification is 353 passing
tests (83 core, 230 view, 1 fixture, 39 app), two installed-font tests ignored;
workspace all-target check, warning-denied Clippy, formatting, whitespace checks
and five Python harness tests pass. Crusty validation reports no new/worsened
findings against the 21 baseline advisories.

The same release passes the isolated fixture-17 continuous-scroll gate at
**108.0 FPS / 5.31ms draw p99**, with 818 input-latency samples over ten measured
seconds (`spec-rich-panels-final-scroll.json`; 1728×1080, 166.7% display scale,
120Hz, 12-second startup wait). This is a >60 FPS gate pass, not a physical-display
measurement or proof of the separate 8ms/50ms planning budgets. Native checks use
only harness-owned Weston, private state/clipboard/bus and synthetic copies.
The current 360×1400 native end-of-table capture
`spec-rich-panels-final-narrow-end.png` was visually inspected: horizontal
scrolling reaches the complete long-code suffix, both Copy controls and the
neighboring explanation column; panels and controls remain within their cell
boundaries. This is reachability/paint evidence, not a clipboard-action test.

The narrow toolbar and nested-table placeholder findings from this pass are
addressed by the subsequent evidence above. Further HTML table review still
needs to cover presentational `align` attributes and spanning-cell geometry in
the inert adapter, together with design-system cell padding and safe overflow.
Full AT-01–20, RTL, native CJK IME and AT text/table interfaces, font/resource
completion, and measured planning budgets remain governed by the full contract.

### Rich table cells: ownership, local padding, and numbering

Fixture 35 covers quotes, bullet lists, and ordered lists inside HTML table
cells; a table inside a quote with another quote inside its final cell; and
ordered lists on both sides of a table boundary. Its `[!NOTE]` is literal
authored HTML blockquote text, not an inferred Markdown alert.

The initial native capture (`spec-rich-cells-before.png`, release `5cb8cdde…`)
showed cell-local indentation shifting whole rows and missing list markers.
The geometry regression failed with a 12px quote text inset instead of 36px.
The read-only projection now records each table's inherited context, cell-local
container ownership, and distinct container start/end IDs. Measurement and
positioning separate inherited table insets from cell-local insets; outer
container spacing stays outside the row, while local quote spacing stays inside
its cell. Geometry cache keys include these derived boundaries/insets. Paint
clips cell-local panels and list markers at cell borders, retaining local
horizontal-scroll offsets without clipping markers at the text origin.

A further ordered-list regression failed at 24px instead of the required 32px
local inset. Table insets now include the shared extra number-rail gap for both
inherited and local ordered lists. The existing Steps presentation's extra 8px
padding remains distinct. The editing regression covers 15 leaf targets in
fixtures 34/35: exactly the edited row's segments are rebuilt, all following
geometry matches the full-render oracle, selection survives, and exact undo
restores the source. Source-only geometry operations do not enter history.

The native-desktop skill's running-app review caught an additional semantic
failure that geometry tests missed: `ol start="9"` rendered as 1, 2 at 360px
(`spec-rich-cells-final-narrow.png`, release `cd883876…`, before the importer
fix). HTML conversion now retains decimal starts, tested at 0, 9, 42 and
999999998. Negative/oversized ordinals, reversed or non-decimal numbering, and
per-item `value` attributes retain authored HTML instead of being renumbered;
such fragments do not offer lossy text conversion. Tables containing these
unsupported list semantics retain their whole HTML source. The inert adapter
also retains `ol`'s `reversed`/`type` attributes. The implementation follows the
[HTML ordered-list semantics](https://html.spec.whatwg.org/multipage/grouping-content.html#the-ol-element)
and [Markdown's nine-digit marker limit](https://spec.commonmark.org/0.31.2/#list-items).
Both new import regressions failed before the fix and pass afterward.

The subsequent native capture (`spec-rich-cells-verified-wide.png`, release
`add9a062…`) correctly showed 9 and 10, but exposed a clipped enclosing stepper
badge at the final table. Marker geometry now positions an inherited list
marker before the table's leading edge, outside cell clipping. A cell-local
list no longer inherits its ancestor's Steps presentation. The dedicated badge
regression failed before the fix and now passes at 75/100/125/200% zoom.

Earlier release `75ffae5b…` passed real native paste of 59 bytes inside the quoted
cell, two-second focused idle, 120% zoom and exact source undo
(`spec-rich-cells-edit`). Its typed capture was visually checked: the quote
wraps inside its locked column, its panel grows with the row, and neighboring
cells and following sections remain correctly placed. This predates the
ordered-list additions and is not evidence for their final native rendering.

Current Rust verification: 347 passed (81 core, 226 view, 1 fixture, 39 app),
two installed-font tests ignored; all-target compiler and warning-denied Clippy
checks, formatting, whitespace checks and five Python harness tests pass.
Release `1be4b45ef17d1a5afb4b2716033aefc45bfc5051af9a49cd1e74e77245eb9ab5`
was visually inspected at 1280×1400, 360×1400 and 1280×1400/200% text
(`spec-rich-cells-complete-{wide,narrow,scaled}.png`). Spline Sans Tachyon body
and Fraunces headings use the existing light design tokens; the wide shell
retains Files/Outline, and the narrow shell hides navigation. The actual native
view shows 9/10 in the cell and an intact outer 1 before the final table, with
the inner 3 rendered independently. Native paste of nine bytes into that
numbered cell, two-second focused idle, 120% zoom and exact undo pass
(`spec-rich-cells-complete-edit`). Its editing screenshot visibly retains 9/10
and the caret. Native 768×600 select-all/copy finds nine unique cross-cell
markers exactly once in source order (`spec-rich-cells-complete-copy.copy.json`).
All captures use the harness-owned isolated Weston display, synthetic files and
private clipboard/state, never a user document or the physical session.
The same release passes the fixture-17 continuous-scroll gate at **108.1 FPS /
7.50ms draw p99**, with 827 input-latency samples over ten measured seconds
(1728×1080, 166.7% display scale, 120Hz, 12-second startup wait;
`spec-rich-cells-complete-scroll.json`). This passes the requested >60 FPS gate,
but is not a physical-session measurement or evidence for every planner p95
budget. It is not claimed as a speedup over the earlier release.
The private fixture-27 AT-SPI regression passes on this release as well: 140
nodes, all real headings, table header/cell hierarchy, canonical list order,
formula source, offscreen heading reveal/stable IDs, task action and exact
source/state undo (`spec-rich-cells-complete-at.atspi.json`). This exercises
the existing representative semantic fixture, not arbitrary rich-cell AT
interfaces or the full accessibility acceptance matrix.

These checks do not establish arbitrary rich-cell support: code/opaque HTML
inside cells, alert-title overlays during local horizontal scrolling, deeper
container paint ownership, full AT table/text interfaces, native CJK IME, RTL,
and the remaining acceptance/planning-budget matrix still need verification.
The full specification remains open, including the unresolved conflict between
its layout overrides and the user's explicit automatic-only direction.

### Container boundary spacing stays outside table rows

Fixture 34 exercises a table-only notice, a table-only quote, a notice beginning
with a table followed by prose, and a notice ending with a table. The native
before capture on release `556a5c3294f54fe9caf0c175ff0c55b30067db625fbec34c4892841004be4d94`
shows the first notice growing backward over its preceding heading, unequal
header baselines, and the quote background missing. The renderer regression
also failed: the first table cell started 48px below its row origin instead of
the intended 10px. These failures were not caught by fixture 33's introductions
before each table.

`component_spacing` now owns the quote/notice outer header and bottom insets.
The row positioner applies their maximum once outside the table row and excludes
them from per-column height accumulation. It does not mutate cached line styles,
so repeated positioning remains idempotent and the local editing path can still
recover the row's original flow position. Notice labels use published container
bounds rather than their first cell's text origin. Container bounds reserve
trailing table padding; table-only quote backgrounds are no longer masked to
the first table cell. Source, canonical identities and Markdown history remain
owned by the existing editor.

The regression checks equal 10px top/20px total padding for every short cell,
all four tables, no backward overlap of preceding headings, complete trailing
border enclosure, unchanged source and idempotent positioning. A second actual
editor test inserts wrapping text into six first/last table-cell cases: exactly
the two-cell edited row is rebuilt, all following geometry matches the full
renderer oracle, selection is retained, refresh does not change source, and
undo restores the entire original document.

Release `5cb8cdde11a7fe38dda91cde2cf2cb9bce95cc64b350a625e6386e810159e827`
was visually inspected at 1280×1400, 360×1200 and 1280×1400 at 200% text
(`spec-container-spacing-{wide,narrow,scaled}.png`). Headers, panels and following
prose are correctly separated. These remain representative captures, not the
full RTL/viewport matrix. Native paste of 45 bytes into the Setting header,
two-second focused idle, 120% zoom and exact undo pass (`spec-container-spacing-edit`).
The typed capture shows a genuinely wrapped header with unchanged column width;
the 120% editing capture shows the legal font-change remeasure and retained
caret. Its private clipboard connection closes at compositor teardown after
the checks have passed. Native select-all/copy at 768×600 verifies six distinct
markers once each in source order across the four containers
(`spec-container-spacing-copy`).

All 340 Rust tests pass (79 core, 221 view, 1 fixture, 39 app; two installed-font
tests ignored), along with five Python tests, workspace/all-target check,
warning-denied Clippy, formatting and whitespace checks. The native-desktop
skill's actual-render review drove the header/panel fixes in addition to the
Rust geometry tests. Full HTML/IME/AT, RTL, planning budgets and the remaining
acceptance matrix remain open; these spacing fixes do not complete the spec.
The native fixture-17 continuous-input scroll gate on this release passes at
**109.6 FPS / 2.99ms draw p99**, with 829 input-latency samples over ten measured
seconds (isolated Weston 14, 1728×1080, 166.7% scale, 120Hz;
`spec-container-spacing-scroll.json`). This is not a physical-display result or
proof of the full planning p95 targets. The existing private AT-SPI fixture-27
regression also passes: real headings, table header/cell hierarchy, list order,
formula source, offscreen heading reveal and stable IDs, task action and exact
source/state undo (`spec-container-spacing-at.atspi.json`).

Next boundary to verify: the canonical model permits `TableCell.blocks`, so a
quote/notice can be *inside* a rich cell as well as contain a table. Fixture 34
covers only the latter. Current `component_spacing` and inherited indentation
derive from leaf context; distinguishing cell-internal container geometry from
table-external geometry needs its own regression before claiming arbitrary
nested-cell support. The native checks above do not establish that case.

### Nested table ownership, cell geometry and container bounds

Fixture 33 adds authored tables inside a quote, a list item and a notice, plus
an independent comparison table. The background worker previously selected
top-level group IDs as table measurement keys, so a scoped nested table received
zero measurements. It now resolves the active groups to their descendant table
IDs. A failing regression now passes for each of the three containers, measuring
exactly the selected table and comparing its constraints with a full native-font
measurement oracle. Layout does not change the original source.

Structural refresh also includes a rectangular table selection's head cell when
there is no text caret or layout focus. The fixture-17 regression inserts a row
in the last table while the viewport remains at the beginning: the selected
table receives exact geometry, at most six tables are measured, the rectangular
selection is retained, and undo restores the entire original document.

Native before/intermediate screenshots exposed a separate rendering issue:
ancestor indentation was added inside each table cell instead of once outside
the table. Intrinsic measurement included quote/list indentation but omitted
the notice inset. The table border therefore escaped the container's leading
padding; notice text could be clipped by inconsistent column constraints.
The table now owns its inherited origin and reduced local viewport, while
columns consistently own 12px left/right padding. The same calculation supplies
wrapping, row positioning and zoom-scaled horizontal masks. Narrow technical
tables can still overflow locally without shrinking text. Container bounds now
include complete table borders, rather than ending at the last cell's text.

The new geometry regression was observed failing with a 36px cell inset instead
of 12px. It now verifies all three nested tables at 304/1000px canvas widths,
matching measured and painted text widths, actual shaped-line fit, container
indentation, complete bottom-border enclosure and source preservation. This
additional geometry work was driven by native-desktop visual validation, not
just the previously passing measurement test.

Release `556a5c3294f54fe9caf0c175ff0c55b30067db625fbec34c4892841004be4d94`
was visually reviewed at 1280×1400 and 360×1000 logical pixels
(`spec-nested-final-{wide,narrow}.png`). The narrow capture follows a native
horizontal gesture: the right-aligned Value column is reachable while the table
remains clipped to its inset viewport. Wide quote/notice backgrounds enclose the
table's complete bottom border. The native-desktop skill's review caught this
enclosure issue after the column-width tests had already passed.

Native typing in the quoted Maximum workers cell, two-second focused idle,
zoom to 120% while editing, autosave and exact whole-document undo all pass
(`spec-nested-final-edit`). Its editing capture shows the inserted character and
retained cell/caret at 120%. The first attempted native edit reached the intended
identifier but failed the harness's literal substring oracle because Markdown
serialization escaped its underscores; this was not counted as a pass. The
follow-up used an unambiguous cell without changing the source/undo assertions.
An attempted ten-step edit zoom was rejected by the harness's existing ±3 limit;
the successful editing case uses two steps, with 200% checked separately.
The 1280×1400 200% reading capture (`spec-nested-final-scaled.png`) visibly
retains the quoted table's full text, alignment and enclosing panel; it is not
a complete all-container 200% interaction matrix. The existing native AT-SPI
fixture 27 also passes on this release: 138 nodes, all real headings available,
table header/cell hierarchy, list order, formula source, offscreen heading
reveal with stable IDs, task action/source update and exact undo. This is the
bounded existing native semantics check, not full AT-SPI Table/Text support.

All **337 Rust tests** pass (79 core, 218 view, 1 fixture, 39 app; two installed-font
tests ignored), along with five Python tests, workspace/all-target check,
warning-denied Clippy, formatting and whitespace validation. The isolated
fixture-17 continuous-input scroll gate passes at **109.8 FPS / 2.78ms draw p99**
on Weston 14, 1728×1080, 166.7% scale and 120Hz over ten measured seconds
(`spec-nested-final-scroll.json`). This is not a physical-display measurement or
proof of the full cached/measured planning p95 budgets. Full RTL, text/table AT
interfaces, native IME and the complete acceptance matrix remain unfinished.

### Structural edits bound exact measurement to relevant windows

A real editor regression on the 22,967-word, twenty-section synthetic fixture
reproduced a synchronous structural-refresh gap: pasting two paragraphs before
the first paint measured all 60 tables and made 1,644 wrap requests / 1,964
shaping calls. Ordinary single-node typing already had a local path; structural
paste, HTML conversion and undo were entering the unscoped geometry path.
The counter regression was observed failing before the change.

Structural refresh now remaps the visible interval through surviving canonical
root IDs and includes the current post-transaction caret's planning window.
The retained-layout pass separately records exact edit-geometry windows, so it
does not mistake fresh wraps for a newly optimized row plan. Those windows are
part of geometry-cache identity. Existing compatible measured windows can still
be reused; unvisited windows retain complete source-order estimated geometry.
Table shaping is scoped using real descendant table IDs, including nested tables,
after retained geometry windows are known and before visual lines are built.
The no-scope table helper is now test-only; production owners use explicit scope.

The same cold structural transaction now measures **3 tables, 86 wraps and 159
shaping calls**, with 80 exact segment layouts and 1,444 deferred segments. The
test verifies every one of the 1,524 canonical segments is still represented,
the refresh itself does not change source, and undo restores all original bytes.
A second regression edits a new caret in the final chapter while the viewport
remains at the start: only the first/final windows get exact geometry, intervening
chapters remain deferred, and the new caret's wraps match a full-measurement
renderer oracle. Its initial test fixture accidentally targeted a nested table
cell; it was corrected to the last top-level paragraph, respecting the existing
block-paste contract rather than changing the editor to accommodate the test.

This is a measured reduction in work counts, **not** a latency speedup claim:
the deterministic GPUI test uses the test text system. Projection construction,
root/index scans, estimated geometry, HTML/math preparation and publication are
still potentially document-wide; the <8ms/<50ms p95 full contract is not proven.
The performance skill drove the before/after counters and independent exact-wrap
oracle rather than an assumption based on lower code complexity.

Release `8f39d1fe8af37070be165fb9bfe72779e59fda376c541e0af7bf0abf64f26ea3`
passes actual native Paste as Markdown on fixture 17 at 1280×1000 and 360×1000,
including autosave, two-second focused idle, and exact undo; the narrow run also
changes text zoom to 120% while editing (`spec-structural-scope-{paste,narrow}`).
The harness explicitly sends Ctrl+Shift+V for this new opt-in case, not ordinary
plain-text paste. Captures show both authored inserted paragraphs, a preserved
title and readable narrow wrapping. Native fixture-30 first HTML edit with a
`VISIBLE` paste, image/link retention, blur, one-step exact-HTML undo and decoded
image pixels also pass (`spec-structural-scope-html`); its blurred capture retains
the marker and paired complete figures. Both native clipboard helpers report
their private Wayland connection closing at compositor teardown after success.

The final suite passes **334 Rust tests** (79 core, 215 view, 1 fixture, 39 app;
two installed-font tests ignored), five Python tests, workspace check, all-target
warning-denied Clippy, formatting and whitespace checks. The native C harness
also builds with `-Wall -Wextra -Werror`.

The raw-continuous-input scroll gate passes at **109.5 FPS, 3.36ms draw p99**, with
829 input latency samples over ten measured seconds on isolated Weston 14,
1728×1080, 166.7% scale, 120Hz (`spec-structural-scope-continuous-scroll.json`).
This is explicitly a continuous-input workload, not a before/after speedup or
physical-display measurement. The opt-in `--perf-input continuous` sends raw
6px deltas on the existing bidirectional schedule; the default remains wheel.
Adding it exposed an existing test-driver limit: its stream parser's 15-character
operation field truncated `continuous-scroll` into an invalid integer. Its
buffer and bounded scanf width now allow that already-supported operation;
the first native invocation failed at that exact transport boundary, and the
fixed invocation reaches the app and passes without changing the FPS/input gate.

Three wheel-profile runs (`spec-structural-scope-scroll`, `-scroll-repeat`,
`-scroll-enter`) reported zero qualifying input samples and correctly failed;
none is counted as a scrolling pass. The pinned GPUI profiler only counts input
that directly invalidates a frame. Wheel impulses updating an already-running
momentum chain need not do so, whereas continuous input updates geometry directly.
A synchronous pointer-entry prelude did not change the zero-sample result.
The separate actual-pixel wheel-coast test on fixture 02 passes on this release
(`spec-structural-scope-wheel-coast.json`), visibly progressing then decelerating
and settling after release. No editor motion rule or profiler threshold was
changed for these measurements. Full wheel-input latency attribution remains
unverified, as do the broader planning-budget gates above.

### Gallery image links retain native accessibility

The original AT-SPI tree exposed standalone image roles and descriptions but
discarded enclosing image links. Native fixture 32 failed on the previous
`cae96d55…` release with “Linked gallery image lacks its actionable native link
parent”. The same real-app check now passes. Canonical image IDs, descriptions
and geometry remain intact; a stable synthetic link parent supplies the authored
URL, native Click and ScrollIntoView actions. Its accessible name refers to the
image rather than inventing another caption. Unlinked figures remain images.
Activation reads the live canonical image and uses the existing link resolver;
it neither opens previously forbidden local image files nor bypasses URL policy.
Deleting the image makes a stale action inert; undo restores normal activation.

The new GPUI regression verifies link metadata, image order, heading activation,
unchanged source/content undo, deletion and restored-node activation. All **332
Rust tests** pass (79 core, 213 view, 1 fixture, 39 app; two installed-font tests
ignored), along with workspace check/all-target warning-denied Clippy, formatting,
diff whitespace checks, and five Python harness tests.

Release `adc0c005c84b2306305dc965066ce37fae3acbdce31c3b8ebcbcd7b40a478a05`
passes private-Weston AT-SPI checks at 1280×800 and 360×800, plus 1280×800 with
200% text and 166.7% display scale (`spec-gallery-at-{wide,narrow,scaled}`).
Reports verify two image descriptions exactly once in source order, the authored
`#gallery-destination` URI through the native Hyperlink interface, the linked
image's parent action, the absence of a link on its neighbor, actual activation
revealing an initially offscreen destination, retained image identities and
unchanged source. Initial wide bounds are two equal 488px columns at the same Y;
narrow bounds are 304px-wide images in vertical source order. The reading capture
`spec-gallery-at-reading.png` confirms full uncropped images without added
captions. The native heading/table/list/formula/task-action/undo fixture also
passes on this release (`spec-gallery-at-regression`).

The image-heavy fixture-31 scroll check with native accessibility active passes
at **109.2 FPS, 3.55ms draw p99**, with input samples present: isolated Weston,
1728×1080, 166.7% display scale, 120Hz, ten measured seconds
(`spec-gallery-at-scroll.json`). This is not a physical-display measurement or
a claim that all planning budgets pass. The desktop-design skill informed the
role/name/action and native traversal verification; Context7 documentation and
the pinned AccessKit/GPUI source guided the semantic parent implementation.
Inline-image links inside mixed prose and rendered HTML, full screen-reader
editing/text/table interfaces, and the remaining AT matrix are still separate
work. This closes a demonstrated standalone-gallery accessibility gap, not the
whole specification.

### Direct HTML paste paints the edited text

The suspected stale-text case was not reproduced. The GPUI regression
`direct_html_edit_paints_current_text_before_and_after_blur` checks the actual
painted `ShapedLine` text, canonical source, projection, and exact-HTML undo.
Native fixture 30 now also accepts a bounded private-clipboard paste in the
existing HTML conversion harness. The seven-byte `VISIBLE` marker is visibly
present in both `spec-html-visible-paste-typed.png` and
`spec-html-visible-paste-blurred.png`; after blur the complete images pair.
The native source/link/blur checks and one-step exact-HTML undo pass, followed
by unchanged-source and decoded-image-pixel checks. This adds test evidence,
not a speculative production repair. Release `cae96d55…` is unchanged.

The current suite passes 331 Rust tests (two installed-font tests ignored),
five Python tests, workspace check and warning-denied Clippy, formatting and
diff whitespace checks. Cross-fragment selection and full native IME remain
separate unverified cases; this first-paste test does not establish them.
Crusty validation `task_f729f6a8ee9ebb51` completed for `ctx_10c5c091b56b`:
21 existing advisories, no new or worsened findings.

### Gallery scoring uses display geometry, not bitmap density

Two large landscape figures previously lost legal gallery candidates because
the scorer treated intrinsic image width as a prose-wrapping preference. A
failing measured-renderer regression reproduced the stack at a 992×928 canvas;
a score-level regression separately exposed the inappropriate wrapping penalty.
Galleries now use a 360 logical-pixel soft display-width preference, capped by
the image's intrinsic preference for small originals. This is distinct from the
existing 260px hard candidate floor. Doubling bitmap resolution no longer adds
text-wrapping discomfort. Complete measured footprints, the 70%-viewport-height
gate, no-upscale rendering, and source-order constraints remain authoritative.
Native 1920px review then exposed asymmetric sizing of identical figures: the
text-column vertical-return term rewarded shrinking the first image. Its new
regression failed with 4+8 spans rather than equal spans. Gallery reading jumps
now account for the horizontal gap between complete figures, while text groups
retain their end-to-next-top return distance. Imbalance and height feasibility
still apply. Identical figures no longer gain arbitrary asymmetric emphasis.

For legal gallery alternatives, a stack's normalized relationship-separation
term additionally accounts for the vertical footprint saved by that alternative,
divided by usable viewport height. This uses existing measured candidates and
does not fetch, reshape, crop, or count images to force a layout. The seven
weights and general DP/hysteresis rules remain unchanged. Existing six-figure
tests still select three columns wide and two at medium width.

A further red regression exposed a loading/editing interaction: a provisional
stack held during focus became a permanent preference once dimensions arrived.
Rows now record `decision_provisional` separately from `height_estimated`.
Holding an unresolved gallery for editing also keeps its constrained neighbors
provisional. Repeated focused replans preserve the stack; blur permits measured
gallery selection, then the settled choice participates in ordinary hysteresis.
The diagnostic trace exposes both states rather than labelling exact dimensions
as estimates or presenting a forced temporary layout as an automatic preference.

All **330 Rust tests** pass (79 core, 211 view, 1 fixture, 39 app; two installed-font
tests remain ignored). Workspace warning-denied Clippy, formatting and the five
Python harness tests pass. New measured cases cover 960×360 and 1920×720 images,
wide pairing, narrow and short-height stacks, exact aspect ratios, ordered
coverage, unchanged source, repeated edit locks and post-blur stable selection.
Synthetic fixture 31 adds large paired landscapes followed by smaller originals.
Final release `cae96d55bf0de4d93d304814a10c8f7f15787441d0af1432889e8fb261d96696`
was reviewed at 360, 768, 1280 and 1920px widths (1000px height), 1280×260,
and 1920×1200 with 200% text. Captures and exact measured plans are
`spec-gallery-balanced-{narrow,medium,wide,ultrawide,short,200pct}`. Both large
figures are complete and equally sized when paired; the widest plan uses
632+632 logical pixels. Narrow/short windows stack without a forced fit, and
small originals are not upscaled. A short window naturally shows only part of
the scrolling document; its full figures remain in the measured source flow.

Final native direct editing of fixture 30, blur and one-step exact-HTML undo
pass (`spec-gallery-balanced-conversion`); its converted figures now pair
rather than remaining stacked. Image pixel/source assertions pass after undo.
The preceding `7ce30144…` build also passed native select-all/copy of all eight
fixture-31 image descriptions exactly once in source order. That copy run's
initial click hit a linked SVG and showed the existing unsupported-local-link
message: copying passed, but it does not prove opening original image files.
No link permission was broadened for this layout change. Clean final read
captures are separate from that retained clipboard artifact.

The final image-heavy fixture-31 scroll run passes with input samples present:
**109.8 FPS, 2.08ms draw p99** over ten measured seconds on isolated Weston,
1728×1080 output, 166.7% display scale and 120Hz
(`spec-gallery-balanced-scroll.json`). This is an affected-workload check,
not a physical-display result, a speedup comparison or proof of all long-document
planning budgets. Native design guidance informed the distinction between
logical display width, complete visual figures and prose reading measure.

Crusty validation `task_4c29b55db8ebbab1` completed for `ctx_fdd5fa066b34`, with
21 existing advisories and no new/worsened findings. Source, test, diagnostics
and native evidence were checked separately. Full image-link accessibility,
physical input behavior and the broader spec acceptance matrix remain open.

### Converted figures retain already-loaded dimensions

An editor regression reproduced an HTML-to-Markdown conversion defect: new
canonical figure IDs had no dimension bindings even though their local resources
were already decoded. Rebinding only the paint geometry exposed a second failure:
the refreshed planner still classified both figures as pending. Structural
projection refresh now uses the same source-and-directory binding helper as the
background worker and supplies those dimensions to the edit-locked planner.
This is read-only cache reuse, not a new decode or artificial resource-generation
increment. Undo drops obsolete figure bindings; redo rebinds current IDs, and an
identical relative filename in a different directory cannot borrow dimensions.

Both regression assertions were observed failing before their respective fixes.
The workspace suite passes **327 tests** (79 core, 208 view, 1 fixture, 39 app),
with the same two installed-font tests ignored. Workspace warning-denied Clippy,
formatting, diff whitespace checks and the five Python harness tests pass.
Final release `075300a4a287552f4a576ee548238c10e8629dedcb5da752aee8b5189d3dcea4`
passes private-Weston direct text editing inside fixture 30's `independent`,
blur without source mutation, and one undo restoring exact HTML and image pixels
(`spec-html-image-rebinding-direct-final`). The 360×1000 read capture also passes
unchanged-source/pixel assertions and visual review: both complete images stack
inside the HTML panel, with readable text and no invented captions
(`spec-html-image-rebinding-narrow-final`). Explicit conversion/typing/blur/two
undos passed on preceding build `645d5f57…` before a test-only helper was removed
from non-test compilation (`spec-html-image-rebinding-blur`).

The post-blur converted Markdown uses correctly proportioned full-width figures.
It does **not** become a paired gallery in this fixture. Detailed planner evidence
(`spec-html-image-rebinding-plan.planning.json`) has zero unresolved images and
legal two-column gallery candidates, but their cost exceeds the retained stacks.
This separates the repaired resource binding from remaining composition-policy
tuning; it is not a claim that the full gallery/editing acceptance matrix passes.

The isolated fixture-17 scroll repeat at 1728×1080, 166.7% scale and 120Hz passes:
**109.0 FPS, 3.80ms draw p99**, with input samples present
(`spec-html-image-rebinding-scroll-repeat.json`). The first run reported 108.4 FPS
and 4.32ms draw p99 but zero input samples and correctly failed its gate; that
artifact remains retained and is not a valid scrolling pass. No physical-display
or speedup claim is made. This repair adds no work to the wheel/frame hot path;
structural refresh is still document-wide and remains part of the larger bounded
planning work, not a newly satisfied performance requirement.
Crusty validation `task_8ebfb34185fa9496` completed for `ctx_d5370381ed5a`:
21 existing advisories, no new or worsened findings. The earlier validation
attempt was terminally rejected while native artifacts were still changing;
validation was retried only after those capture processes finished. Compiler,
test and native results above were run separately, not inferred from the audit.

### HTML images in the actual editor

`editor/html_images.rs` now requests source-validated local references for the
visible/lookahead line set through the app's existing `AnyImageCache` handle.
No second path resolver, fetcher or decoder is introduced. One owned background
task swaps already-decoded straight-alpha BGRA pixels into RGBA exactly once per
ready resource. The per-view ready cache is capped at 128 entries / 64MiB, with
at most one 32MiB conversion in flight. Active resources are protected; a set
that cannot fit retains source fallback instead of an eviction/reload loop.
Immutable projection snapshots may retain the preceding resource batch until
publication; the 64MiB number is a cache budget, not a total process-memory claim.

Completion increments the shared resource generation and uses the existing
quiet/max-deadline batch coordinator, edit/IME guards and anchor restoration.
Source-bound projection, group measurement, leaf geometry and published-geometry
keys include loaded raster identity. A changed base directory clears the local
handoff and invalidates pending geometry. Missing/invalid resources cannot cause
partial fragment rendering. Animated HTML images remain an explicit fallback;
this static renderer does not silently choose one animation frame.

The native conversion test found real link loss in adjacent inline images:
the importer retained the link, but all image serialization fast paths skipped
outer styles. Markdown, HTML-boundary and HTML exporters now retain enclosing
links/formatting. The new regression failed before the fix and passes after it.
Media-only HTML containers convert into separate canonical figures; icons inside
paragraphs stay inline. Conversion-leaf metadata distinguishes image descriptions
from painted text while preserving original ordinals, so ordinary text before
and after figures retains verified direct-edit hits without guessing by alt text.

Checks: **326 Rust tests** pass (79 core, 207 view, 1 fixture, 39 app), with the
same two installed-font integration tests explicitly ignored in the default run.
Workspace warning-denied Clippy, formatting and **5 Python harness tests** pass.
A controllably deferred host-cache test drives the actual request/background
handoff, checks exact RGBA including partial/zero alpha, denies remote references
at the loader boundary, and proves source/undo remain untouched. Other tests
exercise memory admission, directory binding and real-worker geometry cache
invalidation followed by unchanged shared-geometry reuse.

Release `500afe5d1c33900332faecdb31a4ef37bb3c5e29c9fef7ad6b2be3ab0b531684`
uses synthetic fixture 30 on private Weston/Wayland state and input buses.
Wide and narrow screenshots show two complete image placements in source order,
with wrapping at 360px. The final native explicit-conversion test preserves both
image sources, alt text, enclosing link and ordinary rich text, then restores
exact HTML with two undos (typing, conversion). The direct text test clicks inside
`independent`, preserves HTML until typing, and restores it with one undo. Image
pixels reappear after undo. Native 1920×1200 / 200%-text rendering preserves source
and paints the image color bands too. The earlier failed conversion artifact is
retained; its assertion was not waived. The comma in the second alt label is
checked in its canonical Markdown-escaped form.

Reports: `spec-html-images-{conversion-final,direct-final}.conversion.json`,
the matching `.images.json` and screenshots, and `spec-html-images-200pct`.
Initial wide/narrow read captures use preceding release `d1070974…`, before the
conversion fix; their exact hashes remain in their reports. Native delayed I/O
timing, full semantic image/link AT interfaces and the full HTML scale/IME matrix
remain incomplete. Converted galleries initially keep a source-order stack while
editing; post-blur sizing/resource remapping still needs its full acceptance case.

### In-memory HTML image handoff (preceding dependency layer)

The inert adapter now describes up to 32 source-order local `img` references
without installing `src` in Blitz. The generated ordinal cannot be supplied by
authored `data-*` attributes. Remote/file/data URLs, absolute paths, srcset and
invalid intrinsic size attributes retain complete source fallback. Relative
parent components are descriptors only and still require the normal host image
loader's policy; parsing/measurement performs no filesystem or network access.

`html/images.rs` validates host-supplied RGBA buffers (dimensions, exact buffer
length and a 32MiB aggregate bound), then installs them before Blitz layout.
Blitz owns their CSS geometry, clipping and painting; there is no image overlay
or second fetch/decode path. The preview cache keys source-referenced blob
identity and dimensions without retaining source pixel buffers. Missing/invalid
resources cannot borrow another document's cached image or silently disappear.
Alt text contributes to accessible text, not a generated visible caption or
an unverified direct-edit target.

The new local-image conversion test first failed on the old sanitizer. It now
checks alt text, source, enclosing link, title, neighboring rich text and exact
undo. It also exposed and fixes a paragraph boundary after a standalone image:
conversion must not merge the following paragraph into that image's line.
Real Blitz pixel tests check red/blue image locations, dimensions, missing
resources, same-size replacement cache invalidation and invalid/oversized input.

Verification: `cargo test --locked --workspace --quiet` passes **318 tests**
(77 core, 201 view, 1 fixture, 39 app); the two installed-font integration tests
remain explicitly ignored in this default run. Workspace all-target compiler
checks, warning-denied Clippy, formatting and `git diff --check` pass. Crusty
context `ctx_97b4c225f050`, validation `task_2228ff4c195d722d`, completed with
21 existing advisories and no new/worsened findings. This is renderer/core
evidence, not native app image-loading acceptance.

At this preceding stage, the ordinary app still called preview
with no supplied image resources and preserved the source fallback. The loader
integration above connects the existing `BoundedImageCache` to visible references,
while this layer established how to hand
already-decoded pixels to background preparation, account for BGRA/RGBA conversion
and retained memory, and invalidate through the existing batched resource/anchor
coordinator. No release or native-performance claim was made for this layer alone.

### Deferring offscreen text shaping

Baseline release `7cc483c66aa99932a035e2cadc9347652698b05f1a7d9f96131fc7fc0795601a`
still shaped 1451 segments in final geometry on initial fixture-17 replans,
despite candidate-window scoping. The clean baseline's initial workers took
81.534 / 75.174ms (`spec-bounded-planning-before-clean.planning.json`). An earlier
capture under competing load is retained separately as `spec-bounded-planning-before`.

Final geometry now uses native text measurement only for completed compatible
planning windows (current visible/lookahead/focused windows plus prior visits).
Other segments retain the existing complete source-order fallback wraps until
visited. Previously measured windows do not revert to estimates on scrolling.
The exact coverage now participates in `PlanGeometryKey`, because a newly
visited plain-prose window changes wrapping even when its slots are unchanged.
Diagnostics separately count `deferred_text_segments`. Whole-worker scope remains
`whole_document`: projection/metadata/position/index work is still document-wide,
and HTML/display-math preparation is not bounded by this text-shaping change.

The real-worker twenty-chapter regression failed before the change with 573
shaping calls for one requested chapter and passes with fewer than 200. It visits
all chapters and compares the complete final geometry with a fresh fully measured
oracle; it also checks offscreen table deferral, identity/order, width/font
invalidation and unchanged source. A new published-geometry regression verifies
that visiting plain prose invalidates fallback wraps even with no slots, then
reuses the resulting shared geometry on an unchanged repeat. **310 regular Rust
tests**, workspace Clippy/compiler checks, formatting and **5 Python harness
tests** pass. Native comparison/authoring/performance evidence follows after the
release checks; this is not yet a complete affected-only publication pipeline.

Release `84caeac1b46e68dd54c5171bddc8acd3a38333091c46fdb1c6c23029a788be82`
was compared with the preserved baseline executable in A/B/A/B order, using
fixture 17 at 1280×1000, 100% text/display scale, two fresh processes per version
and eight explicit warm replans per process. No build or competing storage job
overlapped this comparison. Native font/layout caches start fresh per process;
OS caches are not purged. Initial-worker samples include the first two
automatically requested widths, including discarded work, not only successful
commits. The later automatic warm follow-up is reported in the raw trace too.

| Worker phase | Baseline median / p95 (ms) | Scoped text median / p95 (ms) |
| --- | ---: | ---: |
| Initial width preparation (4 samples each) | 64.164 / 81.534 | 9.338 / 12.862 |
| Explicit unchanged replan (16 samples each) | 1.364 / 1.763 | 1.308 / 1.663 |

Nearest-rank p95; the four-sample initial result is evidence for this workload,
not a universal latency guarantee or a cold-media benchmark. First-pass final
geometry shaping falls from **1451 calls to 7**, with **1444 deferred text
segments** and **78 geometry requests** instead of 1522. The chosen visible
document area is pixel-identical (RGB comparison below the titlebar, excluding
the trailing 16px scrollbar rail). The scrollbar's extent can change as complete
offscreen estimates are refined; no source content is discarded.

Reports: `spec-bounded-planning-{before-clean,before-2,after-1,after-2}.planning.json`.
Host: Ryzen AI MAX+ PRO 395 / Radeon 8060S, 32 logical CPUs, Linux 7.2.3,
Rust 1.98, isolated Weston 14 headless GL, bundled Fraunces/Spline typography.
All captures use private synthetic fixture copies and private state/input buses.

The 360×900 and 1920×1080 / 200%-text fixture-17 screenshots were visually
reviewed for source-order stack fallbacks, heading attachment and contained table
overflow. The ten intermediate zoom replans take 15.49–21.94ms on this fixture
(`spec-bounded-planning-200pct.planning.json`). Fixture 27's native AT-SPI gate
passes all heading availability, offscreen reveal, stable IDs, list/table
hierarchy, task action/exact undo and accessible formula-source assertions
(`spec-bounded-planning-at.atspi.json`). This is not the full AT-09 text/table
interface or screen-reader speech matrix.

Native authoring checks on this release:

- Fixture 12: a 46-byte plain-text paste into the intended compact-table cell
  autosaves, retains the focused column/row arrangement through three seconds
  idle, survives a text-zoom step and blur, and undoes to exact original bytes.
  The reviewed focused capture preserves the narrow cell width while allowing
  its height to grow; the blurred 110% capture uses a legal wider stacked table.
  Report: `spec-bounded-planning-table-edit-plain.edit.json`.
- Fixture 23: click and provisional XKB dead-key preedit preserve original HTML;
  committing é converts the clicked italic text with formatting/link intact and
  one undo restores exact HTML. Report: `spec-bounded-planning-html-compose.conversion.json`.
  This is native XKB compose, not a CJK IME candidate-window proof.

The initial table probe with punctuation failed the harness's raw-substring
insertion assertion because canonical Markdown escaped the pasted punctuation.
That failed trace is retained as `spec-bounded-planning-table-edit.planning.json`.
The assertion and application were not weakened; the subsequent plain-text
probe is a separate passing case, not evidence that the punctuation oracle is
fixed. Full semantic paste verification remains a harness improvement.

Two sequential AT-active scroll runs on the final release pass at **105.5 /
109.4 FPS** and **12.47 / 7.78ms draw p99**, with 3298 native accessibility nodes.
Reports: `spec-bounded-planning-scroll-{1,2}.json`. These use fixture 17,
1728×1080 output, 166.7% display scale, 120Hz isolated Weston GL, 24-second
warmup and ten-second measurement windows. No build/capture overlap or competing
storage job was present. Both exceed 60 FPS and meet the 16.67ms draw-p99 gate;
their variation is retained, not presented as a scrolling speedup claim. This
does not establish the user's physical-device momentum symptom or complete the
full resource-heavy/20-section/editing acceptance matrix.
Crusty validation `task_b5cfc7e47c14886a` for `ctx_5bfbbf752565` completed with
no new/worsened advisory findings or blocking constraints. Direct tests, compiler
checks and runtime gates above were executed separately from this snapshot check.

### HTML disclosure spacing

The native fixture-28 review showed consecutive disclosure borders touching and
the open summary immediately adjoining its body. The real Blitz geometry test
`disclosure_spacing_separates_groups_and_attaches_body_without_double_insets`
failed before the change (`sibling panel gap: 0`). Default user-agent styling in
`document-view/src/html.rs` now separates consecutive disclosure panels by 16px,
adds an 8px open-summary/body gap, and removes the final direct paragraph's bottom
margin so the 12px panel inset is not doubled. These are native preview defaults,
not authored-source edits or per-frame layout work.

The regression passes at 280, 640 and 1200px fragment widths, checks exact source
preservation and closed-height restoration, and proves authored 30px/0px margins
still override the defaults. **309 regular workspace Rust tests**, warning-denied
workspace Clippy, workspace compiler checks, formatting and **5 Python harness
tests** pass.

Release SHA-256
`7cc483c66aa99932a035e2cadc9347652698b05f1a7d9f96131fc7fc0795601a`
passes fixture-28 native AT-SPI checks at 360×900, 768×600, 1280×1000 and
1920×1080 / 200%-text. All four screenshots were visually reviewed: panel
separation, inner spacing, wrapping and the visible focus target remain coherent.
Artifacts: `spec-html-spacing-{360,768-short,1280,200pct}.png/.atspi.json`.
Fixture 20 also passes mouse, Enter, Space and Tab-return activation, exact
closed geometry and unchanged-source checks (`spec-html-spacing-keyboard.disclosure.json`).
These captures use the isolated Weston 14 GL environment, 100% display scale,
bundled Spline/Fraunces and installed font fallback. They do not close the full
HTML semantics, editing or performance matrix.

Fixture 23's native direct-edit check also passes on this release: click inside
`ordinary rich text` preserves original HTML, the first typed character enters
the clicked text with bold/italic content and the reference link retained, and
one undo restores exact source bytes. Evidence:
`spec-html-spacing-direct-edit.conversion.json` and caret/typed/restored captures.
No performance pass is claimed for this spacing release while the competing
load documented below remains present.
Crusty validation `task_806f6bcb2fe43bf2` for `ctx_fe24a1a1764c` completed with
no new/worsened advisory findings and no blocking constraints; direct checks
and the native evidence above were run separately.

### Native HTML disclosure expansion state

The stronger fixture-28 native gate fails on release
`3d6304516a7585e310c0724239b55b4861c7628c555185026290a64ad504231a`:
`spec-html-expanded-before.atspi.json` records `initial_expansion_state: false`.
The native control already set `aria_expanded`; the pinned
`accesskit_atspi_common` **0.18.1** translation omitted that property entirely.
A minimal adapter test also failed: `Some(false)` did not produce Expandable.

The local patch in `vendor/accesskit_atspi_common/src/node.rs` translates
absent / collapsed / expanded into neither state / Expandable /
Expandable+Expanded. The existing state-set diff publishes notifications.
No application content, labels, roles or disclosure-action implementation were
changed; disclosures are not misrepresented as checkboxes or pressed buttons.
The vendored package retains its original source notices and licenses, with
provenance, tests and the removal condition in `TACHYON-PATCHES.md`.

`performance/html_accessibility_check.py` now requires correct state for the
initially closed, authored-open and revealed nested disclosures, plus stable
control and fragment identities. `accessibility_probe.py` subscribes on the
private AT-SPI bus and requires a real `object:state-changed:expanded` event
for the activated control. Both the true (open) and false (close) payloads
are checked alongside unchanged source and correct visible body text.

Release `c232889c5c4d49c7d12c7765d8032a0e06812d2350aae9e04462c9f3327bde00`
passes the strengthened native gate at **1280×1000**, **360×900**,
**768×600**, and **1920×1080 / 200% text**, using isolated Weston 14 GL and
100% display scale. Reports:
`spec-html-expanded-{1280,360,768-short,200pct}.atspi.json`.
The narrow screenshot was visually reviewed for content, controls and focus.
These reports supersede the older `native_expanded_state_available: false`
limitation below; they do not establish full HTML semantics or screen-reader
speech, complete text/table interfaces, or the full AT-09 acceptance matrix.
The adapter's three-state unit regression, **308 regular workspace Rust
tests**, **5 Python harness tests**, formatting, workspace checks and
warning-denied workspace Clippy pass.

The follow-up native pointer/keyboard gate on fixture 20 also passes:
`spec-html-expanded-keyboard.disclosure.json` records mouse-open, Enter-close,
Space-open, Tab/Shift+Tab return-and-activate, exact closed geometry and unchanged
HTML source. The 768×600 and 1920×1080 / 200%-text screenshots were additionally
reviewed; open bodies remain visible/scrollable and focus is clearly outlined.

Two AT-active scrolling runs of fixture 17 on this release **fail** the strict
draw-p99 gate: **72.6 / 81.9 FPS**, **21.77 / 19.07 ms draw p99** (3298 native
nodes, 1728×1080 output, 166.7% display scale, 24-second warmup, ten-second
measurement). Reports: `spec-html-expanded-scroll{,-repeat}.json`. Both average
frame rates exceed 60, but neither satisfies the 16.67 ms tail-latency target.
A subsequent process snapshot showed an unrelated storage process consuming
roughly 2875% CPU; it was not stopped or changed. This is evidence of competing
load, not proof that it explains the failures. Clean matched performance
validation remains open; earlier successful releases do not pass this release.

Crusty validation `task_010e63ccd896d108` completed for `ctx_65a07b5cc69d` with
no blocking constraints or worsened findings. Its one new name-based NodeId
serialization advisory is a false association: the vendored adapter imports
`accesskit_consumer::NodeId`, not `document_core::NodeId`. No domain model change
is justified by that advisory. Direct compiler/runtime checks above remain the
authority; the snapshot validator did not rerun those commands.

### Multilingual HTML fonts and glyphless text hits

The Latin-only Blitz font context produced 20 missing glyphs in a Japanese,
Arabic and emoji regression. `html/fonts.rs` now initializes a shared context
once, retaining the bundled Spline families as defaults and allowing Fontique
to select installed script/emoji fonts. `system-fonts` and `complex-scripts`
enable this path and dictionary segmentation. Authored resource URLs still use
the deny-all provider: installed platform fonts are not HTML font downloads.
Deployment prerequisites and restart-after-font-install behavior are documented
in `packaging/README.md`.

A second regression proved that valid emoji glyphs were shaping but painting
no colored pixels. The CPU adapter did not forward Vello's PNG bitmap-glyph
feature. A pinned, feature-only `vello_cpu` dependency enables it; no dependency
version upgrade was needed. The explicit installed-font tests require the
validation host's Noto CJK, Arabic and Color Emoji coverage; they are ignored
in portable test runs and run separately here. Both glyph-count and colored-ink
tests pass with those installed fonts. Registered bundled font blobs are shared
between preview contexts, verified by source-blob identity.

Native fixture 29 then exposed an editing failure: clicking Japanese left the
caret in the preceding Markdown paragraph. The smallest failing case was
`<p>e&#x301;</p>` (tested with the literal combining scalar). A Parley ligature
continuation has advance and source text but no glyph. Blitz's
`cluster.glyphs().next()?` aborted the native hit walk, causing the ancestor to
win and invalidating the fragment's complete text correspondence. The local
`blitz-dom` **0.3.0-beta.2** patch uses `cluster.first_style()` instead; only
`src/node/node.rs` differs from the published Rust sources. The upstream base,
licenses, rationale and removal condition are retained in
`vendor/blitz-dom/TACHYON-PATCHES.md`.

The direct-hit regression failed before the patch and passes after it. A
separate test caught click positions inside a combining grapheme. Immutable
caret stops now retain only canonical extended-grapheme edges, without removing
the boundary between separate letters in a font ligature. Joined-emoji offsets
are tested too. An added clipping regression proved that native Blitz hits can
escape `overflow:hidden`; the adapter now checks retained text boxes against
resolved ancestor overflow bounds. Covered/hidden/transformed text retains
explicit conversion, while fully visible text inside a clipping container stays
directly editable. Arbitrary CSS clip paths and the full HTML interaction matrix
are not established by these checks. All geometry and caret-stop preparation
occurs during preview construction, not scrolling; added caret-stop memory is
included in the preview cache budget.

Checks on this worktree: **308 regular Rust tests**, the **2 explicit installed-
font integration tests**, **5 Python harness tests**, formatting, workspace
checks and warning-denied workspace Clippy pass. The normal test count is
75 core + 193 view + 1 fixture + 39 app; font integration prerequisites are
not silently counted as portable tests. Workspace doc tests also pass (zero
doc-test examples currently exist).

Release SHA-256
`3d6304516a7585e310c0724239b55b4861c7628c555185026290a64ad504231a`
was visually reviewed with fixture 29 at 360×900, 768×600, 1280×1000 and
1920×1080 with 200% text. Captures are
`layout-previews/spec-html-fonts-ligature-{360,768-short,edit,200pct}.png`.
Japanese, Chinese, Korean, Arabic, accents and color emoji are visible; narrow
text reflows and longer content remains scrollable. These are synthetic local
documents on isolated Weston 14 headless GL, 100% display scaling, bundled
Spline/Fraunces plus the installed Noto fallback faces noted above.

Native click/type/undo checks in `東京から京都へ` pass at **360×900**,
**1280×1000**, and **1920×1080 / 200% text**. Reports are
`spec-html-fonts-ligature-edit{,-360,-200pct}.conversion.json`; caret and typed
screenshots are retained alongside them. Clicking does not change source,
typing inserts at the clicked Japanese position while retaining bold/italic
text and the reference link, and one undo restores the exact original HTML.
This exercises a physical-key insertion into Japanese text, **not CJK IME**.
The fixture-28 private AT-SPI test at 360×900 also passes visible-text,
native disclosure open/close, source preservation and stable fragment identity
(`spec-html-fonts-ligature-at-360.atspi.json`). The native disclosure still lacks
an exposed Expanded state; this is not full HTML accessibility completion.

Two clean release scrolling runs of the synthetic long-spec fixture 17, with
**3,298 native AT-SPI nodes active**, measured **107.6 / 105.7 FPS** and draw
p99 **10.85 / 11.04 ms**. Both pass >60 FPS / <16.67 ms draw p99. Reports:
`spec-html-fonts-ligature-scroll{,-repeat}.json`. Conditions: isolated Weston
14 headless GL at 120 Hz, 1728×1080 output, 166.7% display scale, 100% text,
24-second configured warmup and 10-second measurement, AMD Ryzen AI MAX+ PRO
395 / Radeon 8060S on Linux 7.2.3. No build or other
capture ran during either measured interval. These are not physical-display
measurements, CJK IME acceptance, or proof of the planner's cold <50 ms target.
Crusty validation `ctx_33bb689fb642` / `task_a9b372d8cec52005` completed with
no new or worsened advisory findings. Compilation, tests and native checks above
were run directly; this static, snapshot-labelled report does not substitute
for them or prove the remaining full-spec acceptance matrix.
The suggested `xmllint` check could not run because it is not installed.
The available `appstreamcli validate --no-net` parses the existing package
metadata but exits 3 with `url-homepage-missing`; that unrelated pre-existing
metadata was not changed in this pass. This is not recorded as a clean
packaging validation.

### HTML accessible text follows the resolved preview

The pending fixture-28 regression was genuinely red on the actual Blitz path:
`cargo test --locked -p document-view html_semantics_expose -- --nocapture`
reported `Closed HTML body marker` in the canonical accessible name. The parser
description was readable inert text, but was collected before CSS layout and
included both closed disclosure bodies and `display:none` / `visibility:hidden`
content. The original release also fails the new native gate
([baseline](layout-previews/spec-html-at-before.atspi.json)).

`html/accessibility.rs` now walks the already-resolved inert DOM in source order,
excluding display/visibility-hidden subtrees and all but the first summary of
closed details. It retains inline word boundaries, nonbreaking spaces, hard
breaks and preformatted text. This is an **opaque fragment text adapter**, not
full HTML roles/inline semantics. The immutable preview owns its readable text,
and the existing cache budget counts the additional bytes. No extra parsing,
resource access, transactions or scrolling-time DOM work is introduced.
`editor/accessibility.rs` binds those labels to matching canonical source/IDs
and repairs enclosing quote/list-item/cell labels so the parser description
cannot leak through an aggregate name. Unrenderable preserved source exposes
its original bytes. Copy/conversion text remains unchanged and source-based.

Three new Rust regressions cover actual fragment rendering/visibility, nested
aggregate labels and immutable cache replacement on disclosure changes, and
inline/break/preformatted text plus source-preserving open/close restoration.
All **305 workspace all-target tests** pass (75 core, 190 view, one fixture,
39 app), along with workspace checking, warning-denied Clippy, formatting and
five Python tests. The new Python oracle rejects hidden-content leaks, missing
or duplicated text and absent native actions. Crusty context
`ctx_70374aedaa54` reports no new or worsened advisory findings.

Release `b707b88212a5abe33715ac380bae352b5d8d86c2efb23ea04a126f99a8232b17`
passes `--atspi-html-check` on fixture 28 at 1280×1000, 360×900 and
1920×1080 with 200% text:
[wide native evidence](layout-previews/spec-html-at-after-1280.atspi.json),
[narrow native evidence](layout-previews/spec-html-at-after-360.atspi.json),
[scaled native evidence](layout-previews/spec-html-at-after-scaled.atspi.json).
The private native probe verifies exact source-order visible markers, hidden
marker exclusion, native disclosure activation/open/close, stable fragment
identity, and unchanged synthetic file bytes after both actions. Wide/narrow
screenshots were visually inspected. Direct HTML editing at 1920×1080 and
200% text still preserves formatting/link targets and restores exact HTML in
one undo ([native edit](layout-previews/spec-html-at-edit.conversion.json)).
All captures use the harness-owned Weston seat, private restricted AT-SPI bus
and synthetic copies; no physical-session or user-document interaction occurs.

A clean 10-second warm scroll run on fixture 17 (22,967 words, 20 sections),
with **3,297 native AT-SPI nodes** active, passes at **109.2 FPS / 8.90 ms draw
p99** ([clean report](layout-previews/spec-html-at-scroll-clean.json)). The
protocol uses 1728×1080, 166.7% display scale, 100% text, 120 Hz, 22 seconds
configured warmup and the same Ryzen AI MAX+ PRO 395 / Linux 7.2.3 / Weston 14
headless GL / Rust 1.98 locked-release environment recorded below. No builds
or other captures overlapped the clean run. An earlier run overlapped the
scaled accessibility capture and measured 89.3 FPS / 14.65 ms p99; it remains
separate [contended evidence](layout-previews/spec-html-at-scroll.json), not a
clean comparison or performance regression claim. Neither run measures the
physical display or proves bounded publication for arbitrarily large trees.

Native inspection confirms the pinned AT-SPI adapter does **not** publish the
disclosure expanded state; the report records
`native_expanded_state_available: false`. It is not counted as passing full
AT-09. Other outstanding HTML work includes full roles/inline semantics,
text/selection interfaces, source-order interactive controls beyond the paint
window, and the existing editing/IME matrix. Visual review additionally proves
that the private Latin-only HTML font context renders Japanese/emoji as missing
glyph boxes. Their original Unicode survives accessibility/copy/source, but
visual font fallback remains required; those screenshots are not full AT-12
acceptance. The full specification goal remains incomplete.

### Canonical accessibility outside the paint window

The native AT-SPI baseline on release `2a44be345f077623705280eced94a5858508489437f39ca8b0d736e1be5c7bef`
failed fixture 27: 120 nodes exposed headings only through section 4. Sections
5–7 were absent, not merely offscreen. The red command was:

```sh
python3 performance/capture-layout.py --fixture 27-accessible-document.md \
  --width 1280 --height 1000 --startup-wait 8 --atspi-check \
  --output performance/layout-previews/spec-atspi-current-before.png
```

The root cause was constructing invisible semantic GPUI controls from the
**visible paint-order range**. `editor/accessibility.rs` now derives semantic
roots/descendants in canonical order from all published lines, without expanding
the visual paint range. A per-view cache keys the immutable geometry allocation
with a weak handle and exact width; `Arc::make_mut` dissociates that handle on
in-place edits. Native node templates and source-derived IDs are retained until
geometry changes. Scroll frames change one document transform, not every node's
local bounds. No text shaping or hidden layout controls are created for AT.
The toolkit still clones/publishes the complete synthetic tree when AT is active;
this is **not** a claim of bounded native-tree publication on arbitrarily large
documents. AT-only work is skipped while native accessibility is inactive.

Every semantic node supports native reveal without moving the content selection.
Checkbox activation uses the same canonical `ToggleTask` command as pointer
activation, including source-preserving transactions and undo. This fixes a
second baseline gap: checkbox roles/state previously had no native Action
interface. Table nodes now also provide their row/column counts. No parser,
canonical content ownership, layout choice, or export format changed.

Two added Rust regressions cover complete offscreen heading coverage, unchanged
cache reuse, width invalidation and copy-on-write invalidation; and native-action
implementation preserving selection/source while reveal scrolls, followed by
task activation and exact content undo. Workspace all-target tests pass **302
tests** (75 core, 187 view, one fixture, 39 app); workspace checking, warning-denied
Clippy, formatting, diff checks and four Python tests pass (two geometry evidence
tests and two private-bus safety/lifecycle tests).

`performance/accessibility_probe.py` and `--atspi-check` use a private D-Bus/AT-SPI
session and an isolated Weston seat, app state and synthetic fixture copies.
Only the installed accessibility bus service is permitted to activate; GSettings
uses memory and GIO uses the local VFS. A first narrow run passed its action gates
but failed cleanup after a generic session bus activated GVFS. The whitelist
replaces that generic session setup; it does not change the physical desktop's
services or accessibility preferences. The probe reads actual native roles,
names, hierarchy, object paths, bounds, states and actions. It verifies all ten
headings in exact order, both table header/cell hierarchies, all six list items in
order, and the original formula source under its Math role. It reveals the final
offscreen heading, checks stable IDs after scroll, toggles the original checkbox,
verifies the exact autosaved source delta, sends native Ctrl+Z, and requires exact
source and checkbox-state restoration with stable heading IDs.

Release `cc918b31970f65386ece2296b9e1291bbc3863edce048c2587500159a2e4c4ac`
passes the native action/identity/source/undo checks at 1280×1000, 360×900,
and 1728×1080 with 166.7% display scale plus **200% text size**. The clean narrow
and scaled runs also assert the table hierarchy, list order and formula source:
[wide evidence](layout-previews/spec-atspi-final-1280.atspi.json),
[narrow evidence](layout-previews/spec-atspi-final-360.atspi.json),
[scaled evidence](layout-previews/spec-atspi-final-scaled.atspi.json).
The final heading moves from native window y=3205 to y=773 at narrow width,
without changing identity or source. Wide and scaled screenshots were visually
reviewed; these are accessibility reveal captures, not a new visual redesign.

With AT-SPI active, fixture 17 (22,967 words, 20 sections) exposes **3,296 nodes**.
Two isolated 10-second warm scroll runs at 1728×1080, 166.7% display scale,
100% text and 120 Hz measured **106.8 / 105.4 FPS**, with draw p99
**11.85 / 12.10 ms**. Both pass >60 FPS and <16.67 ms draw-p99:
[run 1](layout-previews/spec-atspi-scroll-17.json),
[run 2](layout-previews/spec-atspi-scroll-17-repeat.json).
Host/toolchain match the published-geometry run below: Ryzen AI MAX+ PRO 395,
Linux 7.2.3, Weston 14 headless GL, Rust 1.98 locked release and bundled fonts.
The harness permits 22 seconds of warmup (including time for native-tree
inspection), records actual binary/fixture hashes, and runs no concurrent
builds or native captures during measurement. These are not physical-display
measurements or an accessibility-independent speedup claim.

The ordinary **10 MiB**, AT-inactive scroll gate also passes at **109.4 FPS,
4.78 ms draw p99** with the same window/display scale and a 10-second measured
interval ([report](layout-previews/spec-atspi-inactive-scroll-10m.json)). Native
captures now always use the restricted private bus, including the inactive
case; they cannot fall back to publishing a test window on the physical
accessibility bus. The probe refuses to run without the private-bus marker.
Crusty context `ctx_bdb6fd8b49de` reports no new or worsened advisory architecture
findings; compiler, tests and native evidence above remain authoritative.

This is partial **AT-09**, not full screen-reader acceptance. The pinned
`accesskit_unix 0.22.1` adapter does not implement the AT-SPI Table interface;
native row/header/cell roles do not substitute for table navigation APIs. The
editor also still lacks a Text/EditableText interface, precise inline-link and
HTML semantic traversal, and complete keyboard/focus/screen-reader workflows.
Formula source exposure is not equivalent to spoken mathematical structure.
Those gaps, bounded publication, the full acceptance matrix and the other work
packages remain open.

### Reusing unchanged published geometry

`editor/published_geometry.rs` retains the current measured geometry behind
shared immutable handles: source-order lines, paint order, component index and
height. An unchanged replan shares these allocations instead of revisiting
every segment, flattening its cached lines and rebuilding the index. There is
no historical geometry cache or second editable document: ownership is limited
to the current view and its in-flight worker. Source/zoom mutations release the
view's reuse handle before copy-on-write; a worker can safely retain its prior
version until the existing stale-result guard accepts or discards its result.

Reuse compares exact canvas/text scale, immutable measurement-service identity,
loaded node image dimensions, ordered canonical root allocation identities,
table measurements and editing locks, math source-reveal state, authored-source
bound HTML disclosure overrides, placements, list treatments, lead and group
relationships. Planner scores, diagnostic state and completed-window metadata
may change without changing renderer inputs. No hash collision or width-bucket
approximation authorizes reuse. Actual changed geometry takes the complete
existing renderer path, including Blitz/formulas and source-order fallback.

The new 22,967-word regression first failed with **1,522 geometry requests** on
an unchanged replan. It now requires **zero segment geometry requests and one
published-geometry reuse**, shared line/index allocations and unchanged source.
It also edits with a retained old worker version, verifies copy-on-write leaves
that old geometry intact, rejects reuse after the edit, compares replacement
geometry with a fresh renderer and verifies exact one-step undo. Fourteen input
variants test invalidation versus permitted diagnostic-only changes, including
fractional width, fonts, 200% scale, table constraints, images, HTML and math.
The native cached-geometry evidence gate now accepts either positive published
reuse with zero segment work or a complete set of segment-cache hits. Missing
work evidence, mixed paths, shaping, eviction and displaced anchors still fail;
Python regression tests exercise both valid paths and eighteen corrupt reports.

Release `2a44be345f077623705280eced94a5858508489437f39ca8b0d736e1be5c7bef`
passes **300 workspace all-target tests** (75 core, 185 view, one fixture,
39 app), warning-denied workspace Clippy, workspace all-target checking,
formatting, diff checks and the Python evidence tests. Crusty context
`ctx_e415d1788641` reports no new or worsened advisory architecture findings.

The clean timing comparison uses the prior `63b9e6f…` release and this release
in A/B/B/A order: twelve explicitly requested committed warm replans per app
run, **24 samples per binary**, the same 22,967-word fixture, 1280×1000 window,
992px canvas, 926px usable viewport, 100% display/text scale and bundled fonts.
Host: Ryzen AI MAX+ PRO 395, 32 logical CPUs, Linux 7.2.3, Weston 14.0.2 headless
GL, Rust 1.98 locked release. The unrelated CPU-heavy test had terminated;
no builds or other native captures overlapped these runs.

Median / nearest-rank p95, milliseconds:

| Stage | Before | Published geometry reuse |
| --- | ---: | ---: |
| Whole worker | 4.261 / 4.567 | 1.760 / 2.161 |
| Final geometry | 2.458 / 2.667 | 0.038 / 0.047 |
| Projection | 0.834 / 0.945 | 0.737 / 0.984 |
| Candidate planning | 0.845 / 0.975 | 0.821 / 1.057 |
| Commit | 0.156 / 0.200 | 0.094 / 0.144 |

Whole-worker IQR is 0.327 / 0.465 ms; all 24 new warm samples report one
published reuse, zero segment visits/shaping and zero anchor displacement.
The cached-worker <8 ms p95 target is met on this workload. New cold/initial
workers still take **76.368–93.447 ms**, so this is not a <50 ms cold/measured
reflow claim. [Clean statistics and raw-report links](layout-previews/spec-published-comparison-clean.json)
retain distributions and protocol. Earlier [contended results](layout-previews/spec-published-comparison.json)
are separate, not substituted for the clean comparison. The earlier fractional
scale captures exposed 0.2px initial canvas rounding variation even between two
runs of the old binary; the clean comparison uses identical exact dimensions.

The [clean before](layout-previews/spec-published-before-clean-1.png) and
[after](layout-previews/spec-published-after-clean-1.png) screenshots are
pixel-identical. The fractional-scale before-2/after-1 screenshots are also
pixel-identical when their actual canvas geometry matches. Wide table/list
placement and the native extension captures below were visually reviewed.

After two native warm reuse checks, fixture 26's keyboard HTML anchor navigation
opens the required nested disclosures at 360px, keeps the unrelated disclosure
closed, aligns the target at y=54 and preserves exact source
([report](layout-previews/spec-published-html-anchor-360.anchor.json),
[revealed view](layout-previews/spec-published-html-anchor-360-revealed.png)).
Fixture 25's formula edit updates `37/41` to `3x7/41`, autosaves and undoes to
exact bytes ([report](layout-previews/spec-published-math-edit-caret.edit.json),
[edited preview](layout-previews/spec-published-math-edit-caret-typed.png)).
The initial coordinate probe selected after `37` and was rejected by the
within-marker oracle; moving the click one character left passes without any
app change or relaxation of the assertion. Only private synthetic copies were
edited. These checks do not establish full CJK/AT-SPI acceptance.

One 10-second warm scroll gate per workload, after the CPU-heavy external job
finished, passes at 1728×1080, 166.7% display scale and 120 Hz:
**109.2 FPS / 5.69 ms draw p99** for the
[long specification](layout-previews/spec-published-scroll-17-long-layout-spec.md.json),
and **109.4 FPS / 4.33 ms draw p99** for the
[formula stress fixture](layout-previews/spec-published-scroll-24-formula-stress.md.json).
These are isolated-compositor gate results, not a measured scroll-speedup or
physical-display guarantee.

This is **not complete bounded publication**. Projection construction, group
analysis, input comparison and commit anchor lookup still scale with document
size. A changed placement or renderer input still rebuilds final document
geometry using the bounded segment cache. Persistent per-window geometry and
affected-only publication remain required by §17. No editing, accessibility or
content coverage requirement is waived by the warm reuse result.

### Authored HTML IDs and disclosure reveal

The inert sanitizer carries supported elements' authored IDs in an escaped,
reserved `data-tachyon-anchor` attribute, never as DOM/native IDs. Authored copies
of that reserved attribute, scripts and handlers remain excluded; the resource
provider still denies all requests.
Blitz preparation retains source-order targets, closed ancestor ordinals and
actual block/inline fragment rectangles. This includes wrapped inline spans;
positions are not inferred from duplicate labels or parent-block estimates.
Anchor payloads count in preview and geometry-cache budgets. No DOM parsing,
layout, network loading or rasterization was added to link/scroll handlers.

Document navigation resolves competing Markdown headings and HTML targets in
source order, opens only required closed ancestors, and scrolls after the
generation-checked geometry commit. An intervening scroll, selection/edit,
pointer action, other disclosure command or changed source invalidates the
pending jump. Open overrides remain per-view; Markdown bytes, authored `open`
attributes and content undo are unchanged. Unsupported/oversize or transformed
geometry retains source access rather than fabricating a target position.

The initial native-editor regression failed because the Unicode ID did not
resolve. It now verifies both nested disclosures open, the target reaches the
viewport top, and source/undo remain intact. Additional tests cover duplicate
HTML IDs versus heading anchors, required versus unrelated disclosure ancestors,
inline rectangles, hidden targets, and interrupted navigation. A second issue
found during verification was opaque projection offsets choosing a later text
node as the caret. Navigation now uses the retained verified HTML text mapping
when available; otherwise it retains the prior caret. Typing at a verified
duplicate HTML/heading destination and exact one-step undo are tested.

All **298 workspace all-target tests** pass (75 core, 183 view, one fixture,
39 app), along with warning-denied workspace Clippy, all-target checking,
formatting, harness syntax and diff checks. Crusty context `ctx_b75247e5b41f`
reports no new or worsened architectural findings. Fixture `26-html-anchors.md`
contains a percent-encoded Unicode ID, two closed ancestors, an unrelated closed
body, and a source-authored colored target for native pointer/keyboard checks.
Full assistive-technology traversal, arbitrary opaque-fragment link hit regions,
find-in-document reveal, and source-only/empty-anchor navigation remain separate
acceptance work; this does not complete the full goal.

Release `63b9e6f5941f87375b7ab468a49dd8b1eb29d078fc1c94d818a19d5cafe2bc7a`
passes native [Ctrl-click at 1280×1000](layout-previews/spec-html-anchor-1280.anchor.json),
[Alt+Enter at 360×1000](layout-previews/spec-html-anchor-360.anchor.json), and
[Ctrl-click at 1280×1200 / 200% text](layout-previews/spec-html-anchor-200pct.anchor.json).
The `.anchor.json` reports verify the target was hidden before navigation,
became visible near the viewport top, and source bytes stayed exact. Matching
`-before.png` / `-revealed.png` captures were visually inspected: target styling
and text remain intact, and the unrelated disclosure stays closed. These are
private synthetic fixtures on isolated Weston with bundled fonts; they do not
prove AT-SPI traversal or behavior on the user's physical input device.

Two 10-second warm scroll runs of the unchanged 64-fragment HTML fixture on
this release pass the >60 FPS / <16.67 ms draw-p99 gate at 1728×1080 logical px,
166.7% display scale and 120 Hz: **109.2 / 109.2 FPS**, draw p99 **5.80 / 5.20 ms**
([run 1](layout-previews/spec-html-anchor-scroll-1.json),
[run 2](layout-previews/spec-html-anchor-scroll-2.json)). These are isolated
steady-state regression checks, not a cold anchor-indexing, memory, typing or
physical-display performance claim.

### Image-resource arrival batching

`editor/resource_batch.rs` now coalesces resource-only dimension invalidations
for an 80 ms quiet period, with a 250 ms maximum batching interval during
continuous arrivals. Repaints of the same generation do not extend the deadline.
The editor owns one wake task; dispatch and document reset cancel it. Initial
layout, resize, zoom, explicit layout work and newly exposed planning windows
bypass the image-only delay. Selection/IME and existing in-flight work still take
precedence: the 250 ms cap bounds batching, not total layout completion time.

The native-editor regression failed before batching because the first resource
arrival immediately dispatched reflow. It now verifies that two arrivals at
0/40 ms produce no dispatch at 119 ms and one dispatch at 120 ms, applying both
actual dimension pairs together. The same test verifies source bytes, absence of
content undo, reading-anchor preservation and immediate resize bypass. A second
editor test checks stale initial-resource results are discarded; the commit
path also checks the authoritative shared resource generation in case it changed
after the last render request. Deterministic state tests cover continuing bursts,
maximum wait, repeated frames and consumption without restarting the same batch.

All **293 workspace all-target tests** pass (74 core, 179 view, one fixture test,
39 app), as do warning-denied workspace Clippy, all-target checking, formatting
and diff checks. Release
`9e4ebac98e79f2853ffe8dd0abe38d13a649f943a3356a7f39f5ce7403c6b03b`
was visually checked on the real native gallery at
[1280×1000](layout-previews/spec-resource-batch-gallery-1280.png) and
[360×1000](layout-previews/spec-resource-batch-gallery-360.png): six complete
figures form source-order rows at wide width and a readable stack when narrow.
The matching `.planning.json` reports retain committed measured plans. These
ordinary local-load captures verify eventual rendering, not controlled resource
arrival timing; the deterministic editor test supplies the batching oracle.
The owned-task cancellation contract was checked against pinned GPUI scheduler
source and Context7 documentation. Crusty context `ctx_33ac639d0fad` reports no
new or worsened architectural findings.

Two 10-second warm runs of the unchanged 80-display/80-inline formula fixture
on this release pass the existing isolated-Weston scrolling gate at 1728×1080,
166.7% display scale and 120 Hz: **109.0 / 109.0 FPS**, draw p99
**4.54 / 5.29 ms** ([run 1](layout-previews/spec-resource-batch-scroll-1.json),
[run 2](layout-previews/spec-resource-batch-scroll-2.json)). This is a steady-state
regression check, not an image-arrival latency or speedup claim. The batching
interval is added only for image-only updates on an already measured viewport.

This implements the image-arrival scheduling part of §11/AT-13, not complete
affected-group resource integration or font-completion handling. It adds no
fetching, image decode policy or document state. Full visible-window preparation
and publication bounds remain open. The prior momentum-only turn produced
runtime evidence (wheel and continuous native coast checks passed), but did not
resolve the user's device-specific report or change the broader goal's scope.

### Display math stays out of the scroll rendering path

Display math now retains light/dark `BlockFormula` previews on the first source
line's prepared geometry, as HTML and inline formulas already do. Both the
component image and canvas background use that retained preview/extent; neither
requests the global formula cache during rendering. Invalid formulas retain
the prepared source-only fallback without retrying during paint. Source edits
refresh the owning node and preserve the existing scaled height/undo behavior.
Geometry-cache accounting includes both SVG byte payloads and conservative
pixel extents. The canonical document still owns only source content.

The new actual-window regression failed before this change: a paint frame made
four formula requests after eviction by 80 unrelated entries. It now requires
zero requests through repeated paint frames, scrolling and light/dark changes.
This catches even cache hits on the paint path, rather than depending on an LRU
warmup coincidence. The counter is test-only and thread-local. Additional tests
verify payload accounting, changed-preview identity after editing, fresh versus
retained SVG/geometry parity, and unchanged source. `24-formula-stress.md`
contains 80 distinct display and 80 inline expressions; `25-math-boundaries.md`
includes a wide expression, editable and invalid formulas, and the explicitly
documented mixed-direction source fallback.

The performance harness now reserves warmup through the requested startup wait
plus two seconds (minimum five seconds), and records both values. Default
three-second runs retain the original five-second warmup. The initial
`spec-formula-scroll-before.json` startup probe is not used as a matched
comparison; `spec-formula-scroll-baseline.json` uses the corrected protocol.

Release `e118b465e067fd70c3838ff746d261092b2351bc1dabbc27d98900da03003c5e`
passes two matched 10-second warm formula-heavy runs on isolated Weston 14,
1728×1080 logical px, 166.7% display scale and 120 Hz: **109.4 / 109.2 FPS**,
draw p99 **5.24 / 4.05 ms**
([run 1](layout-previews/spec-formula-scroll-retained-1.json),
[run 2](layout-previews/spec-formula-scroll-retained-2.json)). Both pass the
>60 FPS / <16.67 ms draw-p99 gate. The corrected prior binary measured 107.4 FPS
and 6.55 ms p99. These few sequential runs support the gate, not a statistically
established speedup, cold-start/memory claim or physical-display measurement.

Native fixture 25 was visually reviewed at 360/1280px and 200% text. The
[360px horizontal-scroll endpoint](layout-previews/spec-math-boundaries-scroll-end-360.png)
shows the final `a20 = S` without shrinking glyphs or moving surrounding prose.
[Typing inside 37](layout-previews/spec-math-boundaries-edit-typed.png) changes
the fraction preview to 3x7/41; the
[edit/undo report](layout-previews/spec-math-boundaries-edit.edit.json) verifies
autosave and exact source restoration. The
[copy-order check](layout-previews/spec-math-boundaries-copy.copy.json) verifies
unique formula markers in source order. The release's 289 workspace tests,
warning-denied Clippy, all-target check, formatting and diff checks passed.
Crusty validation (`ctx_10c3bc981614`) reported no new or worsened findings.

Mixed-direction inline math remains incomplete: the current native line-index
mapping assumes logical LTR positions, so merely removing the fallback would
risk incorrect formula placement, selection and caret addresses. This change
does not claim RTL or native assistive-technology acceptance.

### Disclosure body editing and narrow title-bar repair

The semantic HTML converter now handles `details` and `summary` as authored
source-order content boundaries. An explicit conversion retains nested and closed
bodies, inline formatting and links without inventing headings or default
legends. Import, rendering and disclosure toggles still retain original HTML.
Conversion is a content edit: it replaces HTML styling and disclosure controls
with ordinary Markdown, as the Edit text tooltip now explains. Exact undo restores
the original HTML, including authored `open` attributes.

Open authored-summary disclosures now participate in the existing verified
text-selection and atomic first-edit path. The renderer regression initially
failed because Blitz includes the generated summary marker in its inline text.
The adapter excludes only the prefix identified by Blitz's inside-marker
metadata; it still advances through that marker's actual glyph geometry.
Authored arrow characters remain real editable text. Nested summaries,
duplicate text, Unicode, narrow widths, hidden bodies and transforms are tested.
No guessed substring matching, active DOM, new network access or scroll-time
layout was added. Closed/partial or generated-default-summary text maps remain
conservative explicit-conversion fallbacks.

Synthetic fixture `23-disclosure-editing.md` exercises native click, typing,
autosave and one-step exact undo. Release
`d6f3be3206aa8004deb6bd4ccd93ea8dd44dd712a7c9cb26be6084c0646b3c92`
passes direct editing at [360×1000](layout-previews/spec-disclosure-final-360.conversion.json)
and [1280×1000](layout-previews/spec-disclosure-final-1280.conversion.json), plus
[1280×1200 at 200% text](layout-previews/spec-disclosure-final-200pct.conversion.json).
Both retain bold, italic and link formatting and leave source unchanged until
the first edit. Matching screenshot prefixes retain the HTML caret and converted
text views. [Native US International dead-key composition](layout-previews/spec-disclosure-final-compose.conversion.json)
also passes: the preedit leaves source unchanged and one undo restores the
original HTML after committing é. This is not a claim of full CJK or
cross-fragment selection support.

The previously unresolved 360px title-bar bug is repaired by giving the filename
zero intrinsic width and allowing it to grow into remaining space. Menu, zoom
and native window controls keep their space; the full name remains available
through its accessible label and hover tooltip. The native close-button check
failed before this sizing change and now passes at 360 and 1280px, with an
additional 768×420 short-window check. These checks cover normal document state,
not all recovery/conflict banners.
The [360px hover capture](layout-previews/spec-titlebar-final-hover-360.png)
visually verifies access to the complete filename after truncation.

The stale packaging-SVG test dependency is removed: image cache tests now use
a small test-only SVG rather than the app icon. The user-owned PNG packaging
migration and production image handling are unchanged. **Full workspace
all-target tests and warning-denied Clippy now pass**, including the 74 core,
173 view and 39 app tests; all-target checking, formatting and diff checks are
also exercised. Earlier sections retain the historical blocked results.
Crusty validation of the HTML/title-bar and test-fixture changes reports no new
or worsened architectural findings (contexts `ctx_63887e6f3749` and
`ctx_e5e36d2800a3`).

Two 10-second warm runs of the unchanged 64-fragment fixture pass the existing
>60 FPS / <16.67 ms draw-p99 gate on isolated Weston 14, 1728×1080 logical px,
166.7% display scale and 120 Hz: **108.9 / 108.6 FPS**, draw p99
**6.94 / 6.63 ms** ([run 1](layout-previews/spec-disclosure-edit-scroll-1.json),
[run 2](layout-previews/spec-disclosure-edit-scroll-2.json)). Reports retain the
release and fixture hashes and actual input/presentation samples. This is a
warm gate check, not a speedup claim: p99 is higher than the earlier 4.50 ms
sample, and another host Cargo test process was present during observation.
Cold startup, memory, physical-display behavior and comprehensive typing-time
performance remain separate acceptance work.

Remaining full-goal work in the package table above is unchanged: visible-window
planning/publication bounds, complete native accessibility and CJK verification,
resource-state integration, HTML ID/reveal/cross-fragment semantics, math RTL and
formula-heavy gates, and the unresolved automatic-only versus §15 controls
choice. This increment does not complete the adaptive-layout specification.

### Missing and empty HTML disclosure summaries

The inert Blitz adapter now supplies a **Details** UI legend only when the
authored element has no direct summary child. This follows the
[HTML default-legend rule](https://html.spec.whatwg.org/multipage/interactive-elements.html#the-details-element),
not a content-summary heuristic. It is created after source-local ordinals are
collected and only in the temporary rendering DOM. Saved source, copy text and
content history remain canonical. At most two temporary nodes are added per
summary-less details element, within the existing bounded source-node count.
Nested default controls use the existing source-bound overrides and keyboard
behavior. Authored empty summaries are not replaced with invented text; a
minimum line-height-sized summary box supplies a usable activation target.
Visual review also caught the empty marker being absent: Blitz skipped the
inline line containing it. An empty/whitespace-only authored summary now gets
one temporary zero-width text leaf solely to create that line. Its retained
accessible label is captured beforehand, so no spacer enters accessible labels,
copy text or saved HTML. A pixel regression requires the empty summary's arrow
to be painted, not merely a clickable empty rectangle.

A second regression exposed direct text leaking from closed details: Blitz's
closed-body CSS only matched element children. The adapter now clears direct
text values only in the closed temporary DOM, after applying the requested open
state. Opening rebuilds from original source and restores all text. No source
wrappers or editing transactions are introduced. Tests compare closed geometry
against a summary-only fragment and require increased open height with exact
original source and copy text, including text before and after a summary.

The new missing-summary test failed with zero controls before the adapter, and
the direct-text test failed with 84px of closed height instead of the correct
55px. Both pass now. A native GPUI input test also verifies mouse, Enter, Space,
Tab/Shift-Tab and reset on an implicit summary without changing source or
revision. **72 core + 172 view tests** pass. Synthetic fixture
`22-implicit-disclosures.md` includes direct text, missing and empty summaries,
nested controls and authored-open state. The native disclosure harness accepts
this fixture using the same exact body-geometry/source-byte assertions as
fixture 20.

Release `d64160ad20eaf1556149b607c6a613fa4604d1f571e861629aea699e744294aa`
passes native mouse/Enter/Space/Tab-return activation with exact source and
closed-geometry restoration:

- [Empty authored summary, 1280×1000](layout-previews/spec-empty-summary-final-1280.disclosure.json).
- [Default summary, 360×1000](layout-previews/spec-implicit-final-360.disclosure.json).
- [Default summary, 1280×1200 at 200% text](layout-previews/spec-implicit-final-200pct.disclosure.json).

The screenshot prefixes match those reports (`-before.png`, `-opened.png`,
`-tab-return-closed.png`). Visual inspection confirms the empty summary's arrow
and readable narrow body reflow. It also found an **unresolved shell issue**:
the longer fixture filename pushes title-bar controls offscreen at 360px.
The document interaction checks pass, but this is not a complete narrow-window
acceptance pass. Document-view all-target and app-binary warning-denied Clippy,
formatting and harness syntax pass. The existing removed-SVG app-test blocker
remains; no packaging files were changed in this increment.

The same binary passes the existing 64-fragment HTML scroll regression at
1728×1080, 166.7% display scale and 120 Hz: **109.3 FPS, 4.50 ms draw p99** in a
10-second warm run ([report](layout-previews/spec-implicit-final-html-scroll.json)).
This fixture uses explicit summaries; it is a retained-rendering regression
check, not a new default-summary-heavy, cold-start or physical-display claim.

Full HTML semantic/AT-SPI traversal, closed-target reveal/navigation, opaque-body
editing/links, CJK candidate UI, cross-fragment selection and safe resource
integration remain incomplete; this does not complete the full specification.

### Source-preserving HTML disclosures

`editor/html_disclosure.rs` overlays native focusable summary controls on verified
Blitz geometry. Mouse, Enter, Space and the accessibility Click action use one
view-state toggle. Sparse choices are bound to canonical node identity, exact
HTML bytes and source-local disclosure ordinal, not summary labels. Nested and
duplicate summaries retain distinct identities. Authored `open` attributes are
never rewritten; reopening restores authored state. The context command
**Restore authored disclosures** also resets reading choices, including when an
expanded fragment exceeds the bounded preview renderer's limits.

The inert DOM applies those choices before background layout. Retained preview,
raw geometry and group-measurement cache keys include the choices. Input does not
run Blitz layout/rasterization, and scrolling paints retained results. Reflow
keeps the toggled summary anchored unless the user has since scrolled/navigated.
Unrelated Markdown edits retain valid choices; changed source invalidates them.
Composition and nonempty selection prevent a toggle rather than changing the
active text transaction. Hidden, occluded and transformed summaries remain a
static fallback when source-to-screen geometry cannot be verified.

The keyboard regression exposed two defects: Enter originally reached the
paragraph-insertion command, and reverse tab traversal skipped the summary.
Dedicated summary activation bindings prevent the first. The second required
marking the tracked `FocusHandle` itself as a tab stop and keeping the editor's
table/list Tab commands out of native child-control focus. The regression now
checks real mouse/Space/Enter dispatch, focus leaving and returning through
Tab/Shift-Tab, the reset action, unchanged revision and exact source bytes.
Other tests verify nested ordinal state, source binding, cache invalidation,
deferred rendering and summary-anchor restoration. **72 core + 169 view tests**
pass. This is not a full workspace test result.

Fixtures `20-disclosures.md` and `21-disclosure-stress.md` are synthetic. The
first includes closed, authored-open and nested disclosures; the second has 64
unique fragments, 22 initially open, with no external resources. Native
`--disclosure-check` compares pixels below the summary, so a focus border alone
cannot pass expansion; close must restore exact following-content geometry.
Each capture also checks the copied fixture's bytes. Tab round-trip activation
is now part of that harness.

Release `c11805f03570cc0fe3708c79bc6cfbbf4295b4b4ee9442738b8e70852ba0f7c8`
passes the full mouse/Enter/Space/Tab-return check on isolated Weston 14, with
bundled fonts and 100% display scale:

| Case | Retained report |
| --- | --- |
| 1280×1000, Files/Outline visible | [Wide](layout-previews/spec-disclosures-keyboard-1280.disclosure.json) |
| 360×1000, collapsed navigation | [Narrow](layout-previews/spec-disclosures-keyboard-360.disclosure.json) |
| 1280×1200, 200% document text | [Text scaling](layout-previews/spec-disclosures-keyboard-200pct.disclosure.json) |
| Nested summary, 1280×1000 | [Nested](layout-previews/spec-disclosures-nested-1280.disclosure.json) |

The report names share screenshot prefixes, including `-opened.png` and
`-tab-return-closed.png`. Wide/narrow, nested and enlarged previews have been
visually inspected for readable content, bounded frames and focus placement.
No source file changes occur through these reading interactions. Document-view
all-target and app-binary warning-denied Clippy, formatting and Python syntax
pass. `cargo check --locked --all-targets` still fails at the two existing
`image_cache.rs` test references to the removed packaging SVG; that unrelated
PNG packaging migration has not been changed here.

Two 10-second warm HTML-heavy scroll runs on that release pass at 1728×1080,
166.7% display scale, 100% document text and 120 Hz isolated Weston refresh:
**109.2 / 109.2 FPS**, draw p99 **5.32 / 5.01 ms**. The fixture is the 64-fragment
synthetic document (SHA-256
`2466acf82ae4d4f16489de4254b2bd5718f7a395398cb2b20e7ed62f094735c9`).
Reports retain actual input/presentation samples, binary and fixture hashes,
configured dimensions and scale: [run 1](layout-previews/spec-disclosures-html-scroll.json),
[run 2](layout-previews/spec-disclosures-html-scroll-2.json). Reproduce with:

```sh
python3 performance/capture-layout.py --fixture 21-disclosure-stress.md --width 1728 --height 1080 --scale 200 --perf-seconds 10 --output /tmp/tachyon-html-scroll.json
```

These are warm retained-rendering results after the existing five-second
measurement warmup, not cold HTML layout/resource timing, memory-budget proof,
20,000-word planning latency or physical-display measurements.

Default/no-explicit-summary controls are covered by the later increment above.
Still incomplete: navigation that reveals
targets inside disclosures, direct body editing/links in opaque details, the
full HTML semantic/AT-SPI tree, CJK candidate UI, cross-fragment selection,
resource integration and the full specification's acceptance matrix. These
controls do not complete HTML authoring or WP-07.

### Native heading and local document links

`document-core/src/links.rs` resolves authored URLs without I/O. It reuses the
existing URL library and percent decoder for file references and Unicode
fragments, and Comrak's GFM anchorizer for exact, source-ordered heading IDs.
Duplicate headings and suffix collisions resolve to distinct canonical NodeIds;
both ATX and Setext headings participate. No headings or HTML IDs are invented.

The editor now routes HTML, Markdown and linked-image destinations through one
policy: HTTP/HTTPS/mailto use the platform handler; `#fragment` navigates to a
canonical heading; local `.md`/`.markdown` files emit a native open request.
Network file hosts, other protocols, control characters and non-Markdown files
are rejected. Relative links need a known document folder. Unknown/unsupported
targets remain source-preserved and HTML link addresses remain copyable.

The app uses its existing guarded document loader, shared sessions and recovery
workflow. The requested fragment travels with that asynchronous load, so a stale
or failed load cannot apply it to the current document. Already-dirty documents
still require saving/resolution before switching files. Exact current-file
paths with fragments navigate without reloading; symlink aliases still enter
the normal guarded open path. Missing headings/files use existing error UI.
Ordinary HTML clicks remain text editing; Ctrl-click, Alt+Enter and the context
Open command navigate without conversion. Heading navigation intentionally
moves the canonical caret; web navigation leaves the current selection intact.

**72 core + 166 view tests** pass, including URL/path policy, encoded names,
Unicode/duplicate/nested headings, exact source preservation, real Alt+Enter
dispatch, destination typing and one-step undo. Core/view all-target and
app-binary warning-denied Clippy pass, as do formatting and harness syntax.
Full app tests remain blocked by their pre-existing references to the removed
packaging SVG; this is not a full workspace-test pass. Crusty validation
`ctx_7cca40b4660f` reports no new/worsened advisory findings.

Release `98171907a9acb3c64cc252ad7a0e1ac18ad799008a29969eef5f3954e92808ca`
was verified on isolated Weston 14 with bundled fonts, 100% display/text scale,
and private copies of synthetic fixtures 18 and 19. Every native check performs
Ctrl-click, verifies unchanged origin/destination bytes, types at the exact
destination heading, and requires one undo to restore exact bytes:

- HTML duplicate-heading navigation at 1280×1000 and 360×1000, including the
  second wrapped link line on narrow screens:
  [wide](layout-previews/spec-native-heading-1280.navigation.json),
  [narrow](layout-previews/spec-native-heading-360.navigation.json).
- Local document navigation at both widths, decoding a space in the filename
  and `é` in the heading anchor:
  [wide](layout-previews/spec-native-local-1280.navigation.json),
  [narrow](layout-previews/spec-native-local-360.navigation.json).
- Markdown duplicate-heading navigation at 1280×1000:
  [report](layout-previews/spec-native-markdown-1280.navigation.json).
- Inspected [wide source](layout-previews/spec-native-links-1280.png),
  [narrow source](layout-previews/spec-native-links-360.png), and
  [local destination/caret](layout-previews/spec-native-local-1280-destination.png).

This advances anchor/source-fidelity coverage; it does not complete AT-07 or the
full acceptance matrix. Opaque HTML IDs/disclosures, full AT-SPI link semantics,
CJK candidate UI, HTML-heavy resource/performance gates, and the other full-goal
work listed above remain incomplete. The previous web-only evidence below is
historical and is superseded by this routing implementation.

One uncontended general-document scroll regression on this release passes at
**108.0 FPS, 7.72 ms draw p99**: 10 MiB generated Markdown, isolated Weston 14,
1728×1080 at 166.7% display scale and 120 Hz, ten measured seconds
([report](layout-previews/spec-native-links-scroll-10m.json)). This is not an
HTML-heavy or physical-display performance claim.

### HTML links without implicit text conversion

`document-core::editable_html_text_leaves` exposes immutable inspection values
from the same semantic conversion used by the first text edit. The renderer
retains link ranges/targets only where its complete source-to-Blitz-glyph map
has already been verified. Repeated labels use canonical conversion ranges,
not label-string matching. Wrapped/styled links keep their range and per-line
hit regions; blank panel areas do not snap to an active link. Hidden or
transformed content with unverifiable geometry exposes no link hit region.
The bounded geometry cache accounts for the additional retained link metadata.

Ordinary clicks still place the temporary HTML text caret. Ctrl-click opens a
web/email target without moving selection or converting source; Alt+Enter at
the caret invokes the link action. The source-local context menu offers Open
link and Copy link address alongside Edit text here. Hover reveals the target
and commands in a wrapped, window-constrained native tooltip. A composition
in progress blocks navigation. The renderer remains inert: URL attributes are
still stripped from its DOM, and no navigation/network provider is installed.
Only HTTP, HTTPS and mailto targets without controls/whitespace are dispatched
from HTML; unsupported targets remain copyable with an explanatory menu label.
This does not add local-file/fragment routing or change existing Markdown's
platform URL route.

The Ctrl-click regression failed before the input handler change (the click
moved the temporary caret instead of opening the link) and passes after it.
Tests cover repeated Unicode labels, styling boundaries, wrapping, exact
decoded targets, blank areas, hidden/transformed content, blocked protocols,
platform URL dispatch, actual Alt+Enter key-event dispatch through the focused
GPUI editor (with a test URL handler), subsequent typing and exact undo.
All **69 core + 164 view tests**, formatting, warning-denied view/all-target and
app-binary Clippy, Python harness syntax and diff whitespace checks pass.
Crusty validation `ctx_f1936eade2de` reports no new/worsened advisory findings.
The unrelated removed-packaging-SVG app tests remain outside this test pass.

Final release `c0fea7e942ee374a19d7da5cb7cd9cfbf4c5192d04bacb66ba372b3a92073440`
was exercised on copied synthetic fixture 10, isolated Weston 14, bundled fonts,
100% display scale, Files/Outline visible where width permits:

- 360×1200 at 100% text: native hover and keyboard selection of the context
  Copy link command. The inspected tooltip and menu fit the window; the exact
  address is copied on the private seat and source bytes remain unchanged.
  [Report](layout-previews/spec-html-links-360-final.link.json),
  [hover](layout-previews/spec-html-links-360-final-hover.png),
  [menu](layout-previews/spec-html-links-360-final-menu.png).
- 1280×1200 at 200% text: the same native copy/source-preservation gate passes
  against the scaled link hit region.
  [Report](layout-previews/spec-html-links-200pct-final.link.json),
  [scaled menu](layout-previews/spec-html-links-200pct-final-menu.png).
- 1280×1200 at 100% text: direct click inside the link label, native XKB US
  International dead-key preedit and committed é. Click/preedit preserve source,
  the first edit retains the link destination and rich formatting, and one undo
  restores the exact authored HTML.
  [Report](layout-previews/spec-html-links-edit-final.conversion.json),
  [edited link](layout-previews/spec-html-links-edit-final-typed.png).

The harness's link-edit oracle now expects the **edited label with the exact
same destination**, rather than incorrectly requiring the unedited label.
Native copy checks do not navigate to an external site. URL dispatch is covered
by GPUI's test platform; physical browser/desktop integration,
link-level AT-SPI semantics, CJK candidate UI, links in opaque HTML (including
authored disclosures), local/relative navigation and HTML-heavy performance
remain unverified or incomplete. These remain part of the full goal.

One general 10 MiB scroll regression run on the final production binary,
1728×1080 at 166.7% display scale and 120 Hz, passes at **108.8 FPS** and
**6.23 ms draw p99** ([report](layout-previews/spec-html-links-scroll-10m.json)).
No build or other native capture overlapped it. This is an isolated-compositor
general-document gate, not an HTML-heavy or physical-display performance claim.

### Reusable source-local renderer geometry

`editor/geometry_cache.rs` retains each bounded segment's native renderer output
before adaptive placement, external gaps and table-row positioning. Cache hits
restore canonical source offsets (including the containing inline-math range);
formula attachment ranges stay line-local. Layout slots, neighbor-dependent card
padding, group spacing and final positions are applied again by the same renderer.
No rendered node becomes a second editable content owner.

Keys use weak immutable leaf-allocation identity, full projection context and
node range, exact available/text widths, table column count, effective image
height, lead font override, math-source editing state and local presentation
breaks. The first-document-line distinction is explicit. Font/zoom environments
have separate native measurement-service lifetimes. Weak identity prevents
allocation-address reuse without keeping deleted canonical source payload alive.
Read-only HTML previews/formula rasters retain their existing bounded resources.

FIFO admission has amortized constant eviction work, capped at 8,192 entries and
16 MiB of accounted keys, geometry and conservatively charged preview payloads
(not a process-RSS bound). Entries over 1 MiB, over 256 lines, over 16 KiB of
projected segment text or with oversized context/break metadata bypass the cache;
their complete renderer output is still returned. Locks protect cache access
only, never shaping, Blitz, formula construction or geometry remapping.

The long-document regression first failed with **1,642 repeated wraps/shaping
calls** in unchanged final geometry. It now verifies zero repeated wraps/shapes
and identical geometry. Independent uncached segment rendering checks source
offset edits, fractional width changes, quote nesting, font overrides, images,
HTML, inline/display math, editing reveal, zoom and undo. Entry/payload eviction,
weak source lifetime and complete oversized-code fallback have dedicated tests.
The table-row typing regression now requires **one changed-cell wrap and one
geometry hit**; the six-segment peer-row regression requires **one changed-node
wrap and five geometry hits**, with existing geometry/source checks retained.

This removes repeated local layout work while resident, but is not the complete
§17 geometry scheduler: projection/group metadata, flattening cached segment
lines, positioning, component-index construction and publication still traverse
the document. Oversized segments and cache eviction still use normal layout.
Persistent window geometry and affected-only publication remain required.

#### Native geometry-cache verification

Final release `5e7e786c9dc301d7a78b3a543aa7f636509041679e6082aed38cfb855563a227`
passes **69 core + 160 view tests**, formatting, diff whitespace checks,
warning-denied view/all-target and app-binary Clippy, and the release build.
The unrelated app-test references to the removed packaging SVG remain in
`image_cache.rs`; this is not a full workspace-test pass.

On the same synthetic 22,967-word/20-section fixture and 1280×1000 isolated
Weston surface used below, twelve explicitly requested committed warm replans
all report **1,522 geometry requests / 1,522 cache hits**, **zero new wraps,
shaping calls or segment layouts**, and **zero reading-anchor displacement**.
The complete screenshot is pixel-identical to the prior candidate-window
release's screenshot, including shell, tables, list cards and spacing.

Clean-run median / nearest-rank p95, milliseconds:

| Stage | Prior candidate-window release | Geometry-cache release |
| --- | ---: | ---: |
| Whole worker | 74.984 / 81.560 | 4.274 / 5.071 |
| Final geometry and rendered extensions | 73.005 / 79.579 | 2.538 / 2.885 |
| Candidate planning | 0.901 / 1.042 | 0.815 / 1.221 |
| Projection | 0.878 / 1.020 | 0.807 / 0.939 |

These are twelve warm samples per version on the same host, not an alternating
binary experiment or a cold-resource guarantee. The current worker range is
3.227–5.071 ms; commit median/p95 is 0.129/0.206 ms. Cold/initial workers in the
new report still take 91.022 and 63.091 ms. The cached <8 ms p95 target is met
on this fixture; bounded whole-document work and the complete measured-replan
contract remain incomplete. The earlier exploratory new-binary capture overlapped
an unrelated CPU-heavy workspace test and is not the timing comparison above.
That process ended before the clean capture; no builds or other native captures
overlapped it. Host: AMD Ryzen AI MAX+ PRO 395, 32 logical CPUs; Weston 14 headless
GL, bundled fonts, 100% document text, 100% display scale.

Evidence: [clean trace](layout-previews/spec-geometry-after-20k-clean.planning.json),
[unchanged native view](layout-previews/spec-geometry-after-20k-clean.png),
[contended exploratory trace](layout-previews/spec-geometry-after-20k.planning.json).
The native harness now has an opt-in cache-resident warm-geometry gate. It fails
on missing committed samples, any geometry miss, new wrap/shaping work, eviction,
empty geometry work or anchor movement. Eight injected trace regressions were
rejected independently of the real passing trace. Oversized/evicting documents
must not use this flag: their uncached fallback is legitimate.
The subsequent [twelve-sample native gate run](layout-previews/spec-geometry-gate-20k.planning.json)
passes on the final binary. Crusty validation `ctx_11bf3db6afaf` reports no
new or worsened advisory architecture findings.

```sh
python3 performance/capture-layout.py --fixture 17-long-layout-spec.md \
  --width 1280 --height 1000 --layout-trace summary --layout-samples 12 \
  --cached-geometry-check \
  --output performance/layout-previews/spec-geometry-gate-20k.png
```

Native authoring checks on the same final binary:

- HTML click and US International dead-key preedit leave source unchanged;
  committing é inside italic text converts atomically, retains formatting/link,
  and one undo restores exact HTML. This is XKB composition, **not** native
  CJK candidate-window/Wayland text-input-v3 or assistive-technology validation.
  [Report](layout-previews/spec-geometry-html-ime.conversion.json),
  [edited text](layout-previews/spec-geometry-html-ime-typed.png).
- A 46-byte paste inside the compact table autosaves and undoes exactly after
  three seconds focused idle and a native blur. The inspected blur capture
  widens the value column, retains its comparison peer, and places following
  prose below the complete row.
  [Report](layout-previews/spec-geometry-table-edit.edit.json),
  [blurred table](layout-previews/spec-geometry-table-edit-blurred.png).
- The inspected [200% inline-math view](layout-previews/spec-geometry-inline-math-200pct.png)
  retains baseline alignment, full fractions/radicals and room for the following
  line. This is a visual regression check, not full RTL/math accessibility proof.

Two sequential clean 10 MiB native scroll runs at 1728×1080, 166.7% display
scale and 120 Hz pass at **109.2 / 109.5 FPS**, **6.16 / 5.75 ms draw p99**:
[run 2](layout-previews/spec-geometry-scroll-10m-2.json),
[run 3](layout-previews/spec-geometry-scroll-10m-3.json).
Both ran after the unrelated CPU-heavy process ended, with no build or native
capture overlap. The first exploratory run also passed but overlapped that
process's final portion and is not part of the clean comparison. These are
isolated-compositor results, not physical-display measurements.

### Visible/lookahead candidate windows

Candidate work now selects complete bounded row windows around visible content,
one viewport of forward lookahead and a quarter viewport of backward margin.
The focused editing window is included independently. Window lookup uses the
existing paint/component index and binary-searched canonical root intervals;
scrolling within already completed windows does not dispatch another job.

The row catalog is separated from measurement. Offscreen windows do not run
candidate search, and offscreen lists cannot acquire an unmeasured automatic
grid. Compatible previously completed rows/list decisions remain in place;
otherwise the complete canonical content uses a stack. Immutable root identity,
exact canvas/viewport, typography-service identity and resource generation guard
reuse. Changed content, unavailable table constraints, released focus locks and
environment changes invalidate coverage. Table constraint cache hits can restore
offscreen geometry, but misses outside the selected windows do not shape cells.
Gallery readiness inspects already-owned resource metadata without measuring
every offscreen figure. No source mutation, fetch or duplicate editor is added.

Background commits still capture the current reading anchor, including any
intervening user scroll. They no longer invalidate the wheel/coast generation;
the ongoing momentum loop continues from the corrected current offset.

This is **scoped candidate work, not complete bounded reflow**. Projection,
group metadata, offscreen placement maintenance, final geometry and its index
still traverse the document. Diagnostics therefore retain `scope: whole_document`
and separately report `candidate_scope`, selected root windows, visited DP
windows and deferred windows. Full retained window geometry/publication is the
next required performance change, not a waived part of §17.

Two new renderer-preparation regressions verify twenty chapters one at a time:
only the requested chapter's two tables/list candidates are measured; completed
rows remain; all roots form an exact ordered partition; after visiting every
chapter the complete geometry equals a fresh whole-document oracle. Width/font
changes invalidate offscreen coverage. A Unicode table edit outside the visible
request is deferred unless focused; a focused row retains its measured widths,
and undo restores exact source. **69 core + 156 view tests**, formatting and
focused warning-denied view/all-target and app-binary Clippy pass.

Native release `d8a466f14ded5cfac4ef6e8823d08bc5e10cacdfc39f1088100c2dac7bf21da3`
uses synthetic fixture `17-long-layout-spec.md`: 22,967 whitespace-separated
words, twenty explicitly synthetic sections, 60 tables, 148,655 UTF-8 bytes.
It contains no confidential Nudge content. On isolated Weston, 1280×1000,
bundled fonts and 100% text, the visible before/after screenshot is pixel-identical.
All twelve explicit committed warm replans preserve the anchor exactly.

- Candidate windows: **21 → 2 searched**, 19 deferred.
- Candidate group requests: **341 → 18**; list-item requests: **360 → 18**.
- Row/list candidate counts: **242/60 → 14/3**.
- Candidate-planning median/p95: **1.454/1.707 → 0.901/1.042 ms**.
- Whole-worker median/p95: **77.029/89.953 → 74.984/81.560 ms**.
- Final geometry median/p95 remains **73.005/79.579 ms**, with 1,528
  wrap-cache misses in the last warm sample. It is the dominant remaining cost.

Twelve warm samples per binary, nearest-rank p95, no overlapping builds or
captures. Whole-worker ranges overlap; no substantial end-to-end speedup or
visible-window budget achievement is claimed. Raw evidence:
[`before`](layout-previews/spec-window-before-20k.planning.json),
[`after`](layout-previews/spec-window-after-20k.planning.json),
[`native view`](layout-previews/spec-window-after-20k.png).

Native scrolling into the second synthetic chapter schedules only the additional
needed window, retains the earlier completed coverage, and commits with zero
anchor displacement:
[`scroll report`](layout-previews/spec-window-middle-20k.planning.json),
[`second chapter`](layout-previews/spec-window-middle-20k.png).
Native Select All/copy finds the unique first/middle/last traceability markers
once each in source order, including still-unplanned offscreen content:
[`copy report`](layout-previews/spec-window-copy-end-20k.copy.json).
This copy check is not proof of keyboard end-of-document navigation.
The continuous-input coast check still shows decreasing motion and settling
after input release:
[`coast report`](layout-previews/spec-window-coast-continuous.json).

Two sequential isolated 10 MiB scroll runs, 1728×1080 at 166.7% display scale
and 120 Hz, pass at **108.7 / 109.0 FPS**, **8.67 / 6.29 ms draw p99**:
[`run 1`](layout-previews/spec-window-scroll-10m-1.json),
[`run 2`](layout-previews/spec-window-scroll-10m-2.json).
No compilation or other native capture overlapped either run. These are not
physical-display or complete bounded-planning latency claims. Crusty validation
`ctx_05a1226634d4` reports no new/worsened advisory architecture findings.
The native compact-table edit also passes autosave/exact undo after a three-second
focused idle and subsequent blur. The inspected blur capture widens the edited
value column and preserves its comparison peer:
[`edit report`](layout-previews/spec-window-table-edit.edit.json),
[`blurred table`](layout-previews/spec-window-table-edit-blurred.png).

### Reusable measured-group footprints

The native typography service now retains complete group/list-item footprints
and measured table constraints, not only individual wrapped lines/glyph widths. Immutable
canonical allocation identity distinguishes unchanged subtrees from edits and
from another document reusing the same numeric IDs/revisions. Keys retain their
Arcs so allocator address reuse cannot produce a false match. Exact width,
cards/lead/step presentation, relationship gaps, effective table constraints and
fitted widths, local math editing and loaded image dimensions are part of the
group key. Font-family/zoom environments have separate service lifetimes.

Only measurements are cached: candidate enumeration, edit locks, penalties,
previous-layout hysteresis and plan validation still run. No selected layout is
reused merely because text happens to match. Cache locks are released before
shaping or calling another cache. There are at most 512 retained group entries,
512 list-item entries and 128 table entries; oversized groups bypass this cache without changing
eligibility, clipping content or weakening the existing renderer fallback.
Diagnostics distinguish group requests/hits/new measurements and table hits
from individual wrap/intrinsic requests. Flat-list item keys retain the bounded
parent list's immutable identity, so source numbering/indentation context cannot
be confused with a matching paragraph from another list.

The unchanged-table-fixture regression first failed with **222 wrap requests,
396 intrinsic requests and four revisited table constraints**, even though the
individual font-cache lookups all hit. It now asserts zero repeated wrap and
intrinsic requests for candidate measurement, with identical row widths,
heights, templates and scores. A table-cell edit remeasures one table and the
affected groups while retaining the others; geometry is compared with a fresh
typography service. Exact-width changes, undo, distinct documents, loaded image
dimensions and font/zoom environment separation are covered separately.
A six-item list regression also failed with **30 repeated wraps and 60 width
requests**; both drop to zero after item-footprint caching, with the same selected
layout. These tests exercise the native measurement adapter with GPUI's
NoopTextSystem; actual-font measurements are verified separately below.

At this earlier cache checkpoint the background path still rebuilt the whole
projection/geometry and enumerated all planning windows. The scoped-candidate
follow-up above replaces that enumeration policy, but bounded geometry and
publication remain open. No visible-window latency claim follows from the
cache-hit regression or small-fixture timing.

Final native verification uses release SHA
`43f74b14b90b0ff6014e621f621d8bd25e08f58d379ca331061a16d5e47e1264`,
isolated Weston/Wayland and bundled Fraunces/Spline Sans fonts. The 1280×1000
table screenshot is pixel-identical to the clean pre-cache screenshot. Twelve
explicit, committed same-environment samples report no changed rows and zero
anchor displacement. Native warm planning now has **27 group hits, zero wraps
and zero intrinsic requests**; table measurement has **four hits and zero new
constraints**. Final geometry still requests 47 cached wraps. This is evidence
of candidate-measurement reuse, not elimination of the whole geometry pass.

| Small table fixture stage | Before median / p95 ms | After median / p95 ms |
| --- | ---: | ---: |
| Whole background worker | 0.730 / 0.837 | 0.294 / 0.427 |
| Table constraints | 0.144 / 0.173 | 0.015 / 0.024 |
| Candidate planning | 0.465 / 0.524 | 0.125 / 0.222 |
| Final geometry/extensions | 0.063 / 0.080 | 0.109 / 0.152 |

These are 12 warm samples per release, nearest-rank p95, excluding startup and
discarded jobs, with no overlapping builds/captures. The geometry stage was
slower in this run; no geometry optimization is claimed. Raw reports and images:
[`before`](layout-previews/spec-group-cache-before-clean.planning.json),
[`after`](layout-previews/spec-group-cache-after.planning.json),
[`table view`](layout-previews/spec-group-cache-after.png).

The native list fixture reports **45 item hits and seven group hits**, with no
new candidate wraps/width requests, list changes or anchor displacement on all
three explicit warm samples. The inspected screenshot retains row-order grids
and vertical instructions:
[`list report`](layout-previews/spec-group-cache-lists.planning.json),
[`list view`](layout-previews/spec-group-cache-lists.png).

Native table-cell paste (46 UTF-8 bytes inside `64 MiB`) autosaves and one undo
restores the exact original source. The three-second focused capture retains
column widths while the cell grows; the blur capture widens the value column
without stale cached constraints or moving content out of source order:
[`edit report`](layout-previews/spec-group-cache-table-edit.edit.json),
[`focused`](layout-previews/spec-group-cache-table-edit-idle.png),
[`blurred`](layout-previews/spec-group-cache-table-edit-blurred.png).

Two sequential isolated 10 MiB scroll runs at 1728×1080, 166.7% display scale,
120 Hz measure **106.3 / 107.2 FPS** and **8.91 / 8.37 ms draw p99**, passing
the >60 FPS / <16.67 ms gate:
[`run 1`](layout-previews/spec-group-cache-scroll-10m-1.json),
[`run 2`](layout-previews/spec-group-cache-scroll-10m-2.json).
These do not prove physical-display performance, formula-heavy performance or
the §17 visible-window planning budgets.

Final checks: `cargo test --locked -p document-core -p document-view --lib`
passes **69 core + 154 view tests**; `cargo fmt --all -- --check` passes;
warning-denied Clippy passes for core/view all targets and the app binary.
Crusty validation of `ctx_04690a58da34` completed with no new/worsened advisory
architecture findings. This does not replace runtime or full-workspace checks.

### Affected-table typing and transient column locks

Table-cell text edits now reuse the bounded row geometry path. A standalone
table rebuilds only the affected source-order row; a table in a peer composition
rebuilds the complete containing adaptive row. The existing renderer still owns
wrapping, header/body typography, alignment, cell padding and row heights.
Rows containing tables are capped at 512 projected segments and 64 KiB; larger
affected groups retain the general path without dropping content.

The focused table's existing measured (or explicitly estimated) constraints are
held separately from content. Background reflow carries this transient lock at
the same canvas width. Blur, font/zoom changes and changed available width allow
fresh constraints. Editing replaces the cached canonical table and invalidates
its old measurement cache, so those constraints cannot leak into a later unlocked
projection. There is no Markdown annotation, preference or content-undo entry.
The component index now consumes the retained paint order instead of sorting all
visual lines again after an affected-row edit. Index/offset/position maintenance
still traverses following content, and copying a changed table remains dependent
on its size; this is not constant-time editing or complete visible-window planning.

`table_typing_only_wraps_the_affected_row` first failed with **706 wrap requests,
1,015 shaping calls** for one cell before 100 other rows and 500 paragraphs.
It now asserts **two wrap requests**, unchanged column constraints, equivalence
to full geometry and exact undo. The same test covers a 600-row table too large
for full intrinsic measurement. Paired-table header and body-cell growth/deletion
are covered at 100/150/200% text alongside retained inline math following the
edit. A separate preparation test checks same-width background locking, unlocked
remeasurement, Unicode preservation, invalidated cache reuse and zoom release.
All **64 core + 140 view tests** and focused warning-denied core/view/app-binary
Clippy pass. Native/release evidence follows below; the complete background
planner and accessibility/IME acceptance work remain open.

Final release SHA **`5868fe1d26792d7bdd27c0f01995934db437950d49a7aaca3fc1051b7ff59e1d`**
was built with `cargo build --release --locked --bin tachyon` and checked
on isolated Weston/Wayland with the bundled Fraunces/Spline Sans fonts. All
captures use copied synthetic fixture `12-measured-tables.md`, never a user file:

- `spec-table-typing-peer-1280`: 1280×1000, native paste inside the compact
  property's value cell, followed by a three-second idle capture and moving the
  caret to the Configuration heading. The focused table retains its original
  column widths and row pairing as the cell grows. In `-blurred.png`, the value
  column widens and the row becomes shorter after leaving the table. The right
  comparison stays in source order, and following content stays below both.
- `spec-table-typing-explanation-1280`: native body-cell paste in an
  explanation/table pair, stable column widths after three seconds idle and
  following prose below the complete table. `-idle.png` was visually inspected.
- `spec-table-typing-narrow-edit`: 360×900, conventional stacked table body-cell
  edit. `spec-table-typing-200pct-edit`: 1920×1080 with actual 200% text, header
  edit. Both idle captures show natural row growth at retained widths and
  complete text, without moving the table into a new column.

All four `.edit.json` reports verify exact insertion into the selected source
span, native autosave and whole-file byte-for-byte undo. The optional harness
idle capture also asserts that background layout does not change edited source.
`spec-table-typing-before-1280` preserves the previous release's unedited baseline;
`spec-table-typing-{360,200pct}` preserve this release's unedited narrow/zoom views.
These are real native editing/visual checks, not complete IME or AT-SPI evidence.
Layout diagnostics still report **whole_document** for background preparation;
small-fixture worker durations must not be presented as visible-window budgets.

Two sequential final-release 10 MiB mixed-Markdown scrolling runs, after all
compilation and native editing captures finished, passed: **109.4 / 108.1 FPS**,
**6.00 / 7.41 ms draw p99**. Reports:
`layout-previews/spec-table-typing-scroll-10m-{1,2}.json`, both carrying the final
release hash. Environment: isolated Weston 14 headless GL, 1728×1080 output,
166.7% display scale, 120 Hz, ten measured seconds per run, layout trace off.
Machine CPU: AMD RYZEN AI MAX+ PRO 395 with Radeon 8060S; compiler
`rustc 1.98.0 (88d9e12ae 2026-08-18)`, workspace release profile.
These are regression-gate measurements, not a matched before/after latency
claim, a physical-display measurement, or a table-typing latency benchmark.
Final formatting, diff whitespace and harness syntax checks pass. Crusty's
advisory validation (`ctx_f35610abd5e0`, `task_d20ae2e48f4333e9`) reports no new
or worsened architecture findings. The unrelated deleted-SVG packaging test
references still prevent claiming a full-workspace test pass.

### Affected-row typing geometry

Ordinary edits in a lead paragraph or a complete adaptive row now use the same
renderer's bounded segment-range builder. Only that row's text is wrapped and
measured; it retains the current topology, widths, heading/card spacing and
table-peer geometry. Following geometry and source indexes are shifted without
reshaping; only the replacement row's paint entries are sorted. The renderer
checks that the old row is contiguous in paint order before updating it.

`TransactionResult::text_changed_node` explicitly identifies a non-structural
paragraph/heading/code edit. The old dirty-set-size heuristic rejected list
items because their parent lists are also dirty. Structural commands and image
attribute changes cannot claim this hint. At this increment, table-cell changes
and rows over 32 projected segments or 64 KiB retained the general renderer
fallback; the table follow-up above extends that path.
The canonical source, transactions and undo history have not changed owners.

`adaptive_typing_measures_only_the_affected_row` first failed with **521 wrap
requests and 521 shaping calls** when one peer paragraph was edited before 500
unrelated trailing paragraphs (approximately 20,000 words). It now verifies
exactly **six wrap requests**, one per peer-row segment, plus equivalence to a
full geometry rebuild and exact source undo. The scale regression exercises
lead/first and second peer columns/first and second list-grid rows/code and
table explanations at 100/150/200% text. Both growth and deletion preserve the
full-rebuild geometry, paint order, slots, gaps and retained inline-math ranges.
Formula attachment offsets remain line-local; only their containing absolute
line range moves after an earlier edit. All **64 core + 138 view tests** and
focused warning-denied core/view/app-binary Clippy pass.

This removes repeated shaping on the immediate adaptive typing path; it does
**not** yet make all work independent of document length. Projection offsets,
component indexes and following positions still require linear maintenance.
Background reflow still processes the whole document; visible-plus-lookahead
planning, affected-table updates, chapter caches and planning-budget acceptance
remain open. Native captures from release
`179188e352a82bd074173605f431d2cb5fd18dc0cfaf3501d90e0ee128167198`
were inspected at 1280×1000: `spec-row-refresh-peer-1280-typed.png` and
`spec-row-refresh-grid-1280-typed.png` show the edited row growing without
changing columns, with following content remaining below it. Their edit reports
verify native insertion/autosave and whole-source undo. The peer pixel probes
compare before-edit and after-undo surfaces, not intermediate typing geometry.
`spec-row-refresh-200pct.png` is a 1920×1080 native 200%-text visual capture,
not a native editing check at that scale. Separate performance gates below use
the final table-follow-up release, not this superseded binary.

### Measured galleries and linked figures

Consecutive canonical images now use the measured row planner rather than the
old font-free two-column heuristic. A gallery retains its heading, never absorbs
intervening prose, and exposes at most nine internal source-order figure units.
Complete images use source-bound intrinsic dimensions, without cropping or
upscaling. Missing dimensions retain a usable stack. The same width, viewport
height, score, source-partition and editing-lock checks apply as for other rows.

Standalone `[![alt](image)](target)` is now a canonical image with separate
enclosing-link metadata. Image attributes and link editing preserve both titles
and destinations; exact undo restores original Markdown. Full-figure clipboard
selections, including figures at either boundary of a multi-block selection,
retain images and links. Partial alt-text selections keep text behavior. The
existing editor link-target route and pointer affordance now recognize these
figures. This does not claim complete linked-image keyboard/AT-SPI activation.

The first native wide capture revealed a loading-history defect not covered by
the initial all-dimensions-ready test: early known images acquired settled-stack
stability penalties, retaining a fragmented 1+3+2 composition after loading.
`gallery_resource_arrival_does_not_pin_a_partial_composition` reproduced the
same class of failure deterministically (six stacks remained after six staggered
arrivals). A canonical gallery now remains provisional until all its dimensions
are known; partial stacks cannot seed layout hysteresis. This resolves its
composition together without starting any resource loads during measurement.
Existing viewport-plus-lookahead loading is unchanged; an offscreen or missing
resource can therefore keep a gallery stacked until its dimensions are known.

All **64 core + 136 view tests** and focused warning-denied view Clippy pass.
The tests cover linked-image round trips/undo/copy, heading attachment, missing
dimensions, staggered resources, 3/2/1-column geometry, exact primary partition,
complete aspect ratios and the gap after a gallery. The unrelated stale
packaging SVG test references still preclude a full-workspace pass claim.
`spec-gallery-partial-before-{1280,1920}` records the first native captures with
the partial-loading issue.

Final release SHA **`63e07285f10d9f0caf5130bed82627a268c0388498b3953137c1382857d1f0b7`**
was checked on isolated Weston 14/Wayland at 100% display scale, with bundled
Fraunces/Spline Sans and the existing Files/Outline shell (hidden at narrow
widths). Evidence under `layout-previews/`:

- `spec-gallery-1920`: 1920×1080, two three-column rows at 416 logical pixels
  per column; all six complete 400×240 figures and the following paragraph are
  visible. Native select-all/copy preserves the six authored alt markers exactly
  once in source order (`.copy.json`).
- `spec-gallery-1280`: 1280×1000, two three-column rows at 320 logical pixels
  per column; complete figures scale proportionally to the smaller canvas.
- `spec-gallery-1100`: 1100×1000, three two-column rows at 398 logical pixels
  per column, in ordinary row-major source order.
- `spec-gallery-360` and `spec-gallery-768-short`: 360×900 and 768×600,
  readable source-order stacks and uncropped figures. At 360 pixels the columns
  do not fit; at 768×600 the planner scores stacks above the feasible pairs.
  The six gallery dimensions are resolved; the remaining unresolved image is
  the intentionally missing asset in the separate pending-dimensions section.
- `spec-gallery-1920-200pct`: actual 200% text, remeasured two-column rows,
  complete figures and wrapped prose. This is native zoom, not image resizing.
- `spec-gallery-edit-1280`: native insertion inside `A short introduction`
  reached autosave, undo restored the entire fixture byte-for-byte, and two
  independently checked gallery surface coordinates retained the expected RGB
  values through selection/edit/undo (`.edit.json`, `.layout.json`).

These checks are targeted gallery evidence, not full AT-19/AT-09 accessibility
acceptance. No external image links were opened. Planning diagnostics continue
to report **whole_document**, not bounded visible-window scheduling. The full
goal's HTML authoring, math acceptance, scheduling and control-policy gaps remain
as listed above.

Three sequential 10-second runs of the existing 10 MiB mixed-Markdown scrolling
workload, after compilation/captures completed, measured **91.4 / 102.3 / 105.8
FPS** and **21.30 / 13.80 / 9.08 ms draw p99**. Environment: isolated Weston,
1728×1080 output pixels, 166.7% display scale, 120 Hz, trace disabled. Evidence:
`spec-gallery-scroll-10m-{1,2,3}.json`, all carrying the final release hash.
The first run **fails** the <16.67 ms draw-p99 gate; the next two pass. CPU-heavy
unrelated processes were observed before the third run and were not changed.
This is not a clean idle-machine performance pass or proof of improvement;
the variation requires a controlled retest/profiling. All three average-FPS
measurements exceed 60, but that alone does not establish smooth frame pacing.
Final formatting, diff whitespace checks and focused app-binary Clippy pass.
Crusty's advisory validation (`ctx_974729d24dd5`, `task_33d05446e9a16f4c`)
reports no new or worsened architecture findings.

### Measured explanation/image pairs

Background row measurement now borrows the existing loaded image dimensions,
using the same source-bound intrinsic sizing helper as final figure geometry.
No image fetch, decode, or source transaction is initiated by measurement.
Only the source-adjacent explanation/image relationship already recorded by
the group analyzer is eligible. The ordinary candidate widths, height comfort
limit, scoring, source order, and focus locks apply; images remain complete,
keep their aspect ratio, and are not upscaled past their natural size.

Missing, zero-sized, or stale-source dimensions do not authorize a pair.
Unmeasured fallback rows are now explicitly marked estimated and cannot seed
the prior-measured-layout hysteresis preference. An explicit focus lock still
keeps a provisional stack when dimensions arrive during editing. Wide technical
blocks are measured at canvas width; ordinary prose retains its bounded measure.
The first 1920-pixel capture exposed an intrinsic-width accounting error after
that width distinction: the intentional margin outside the prose measure was
counted as useful content width. A new `wide_stack_measurement_excludes_intentional_prose_margins`
regression failed before the correction. Only actual content insets now enter
that width, keeping stack scores independent of intentional prose margins.

The `loaded_image_explanation_uses_measured_geometry_and_pending_images_stack`
regression first failed in the actual snapshot/background-preparation path,
then passed with loaded dimensions. It checks exact measured/rendered image
height, uncropped aspect ratio, following-paragraph spacing, missing/invalid
dimensions, narrow/tall fallback, focus-lock preservation, and unchanged source.
Synthetic fixture `15-measured-figures.md` and `measured-components.svg` cover
the native visual path without external content. All **62 core + 132 view tests**,
formatting, and focused warning-denied view/app-binary Clippy pass. Crusty's
final advisory validation (`ctx_67306a6ad4ed`, `task_0ca7ea90600344b7`) reports
no new or worsened architecture findings. The unrelated stale packaging SVG
test references still prevent claiming a complete workspace test pass.

Final release SHA **`cc362833f3c59f05c7133885b191fd66daed44ebb61b34dca9c0efdaa3197c3b`**
was captured on isolated Weston 14/Wayland, 100% display scale, bundled Fraunces
headings and Spline Sans body, with the existing Files/Outline shell (hidden at
narrow widths). Captures and accompanying content-free planning JSON are under
`layout-previews/`:

- `spec-figures-edit-verified-1280`: 1280×1000, measured 488/488 columns;
  native typing inside the explanation reached autosave, preserved the figure's
  independently checked surface coordinates, and undo restored every source byte.
- `spec-figures-1920`: 1920×1080, measured 524/740 columns, complete 600×180
  figure; native select-all/copy retained explanation, image alt, following
  paragraph, and next section exactly once in source order (`.copy.json`).
- `spec-figures-360` and `spec-figures-768-short`: 360×900 and 768×600 complete
  stacks; text wraps and the image preserves its full aspect ratio.
- `spec-figures-1920-200pct`: 1920×1080 with actual 200% text zoom; the
  remeasured pair still fits, with four readable explanation lines and a complete
  figure. The final capture is paired, not the earlier pre-correction stack.

`spec-figures-before-1280` retains the prior release's source-identical stacked
composition. The first native edit oracle included a period escaped by the
existing canonical Markdown serializer; the verified run targets the unique
punctuation-free `Trace each connection` span and still requires exact whole-file
undo. No edit oracle or application behavior was weakened to hide a wrong caret.
These checks cover this image/explanation increment, not full gallery/link/AT-SPI
acceptance or bounded visible-window scheduling. The timing JSON still correctly
labels background planning as **whole_document**; it is not a visible-window
latency claim.

With the final build and all native captures/compilation finished, two sequential
10-second samples of the existing 10 MiB mixed-Markdown scroll workload passed
the >60 FPS / <16.67 ms draw-p99 gate on isolated Weston at 1728×1080 output
pixels, 166.7% display scale and 120 Hz: **109.0 / 107.7 FPS**, draw p99
**6.57 / 8.36 ms**. Raw evidence is `spec-figures-scroll-10m-{1,2}.json`.
These are release regression gates, not an optimization comparison, a physical
display measurement, an image-heavy stress test, or proof of bounded planning.

### Provisional direct HTML composition

Direct IME now uses the existing canonical composition transaction. Preparation
checks the immutable HTML source and maps the selection into one converted text
leaf without publishing conversion. Updates rebuild from that same baseline,
retaining converted IDs instead of appending successive preedit strings. Commit
adds one history entry containing conversion plus final text. Cancel restores
the exact original HTML and its temporary caret/selection. Empty reset/unmark
cannot leave an unintended bare conversion. Unsupported multi-leaf targets are
rejected before publishing any change.

The initiating editor owns preedit updates, commit and cancellation. Other views
cannot mutate content/history or overwrite the canonical selection while that
composition is active. Switching away from a session or releasing its owning
editor cancels provisional text and advances shared-view generation; another
view's departure does not cancel it. Autosave and recovery continue to use the
existing composition-active guard. No mutable DOM or second editable text store
was introduced. This is part of INV-09, not full native IME/AT acceptance.

Native dead-key verification exposed a real rendering crash that the previous
NoopTextSystem tests missed. `apply_marked_runs` extended nonoverlapping style
runs toward the preedit range, producing lengths beyond the actual string.
The new HTML-preedit regression failed with **105 bytes of runs for 45 bytes of
text**. The fix leaves disjoint style runs untouched and only splits overlaps.
The same native US International acute+E sequence then rendered its preedit,
committed é at the clicked italic text, autosaved, and restored exact HTML with
one undo. The private compositor configuration never changes the user's keymap.
The first native failure's log identified GPUI's out-of-bounds font-run slice;
the black screenshot was a crash, not a successful provisional render.

Verification so far: **69 core + 150 view tests** pass; warning-denied Clippy
passes for both crates/all targets and the application binary. Formatting and
diff whitespace checks pass. Tests cover Unicode preedit replacement/selection,
empty cancellation, undo, stale/invalid targets, duplicate text identity, shared
view isolation and owning-view departure. The post-shaper-fix native 1280×1200
capture is `layout-previews/spec-html-ime-1280-fixed` (`*.conversion.json`,
`*-preedit.png`, `*-typed.png`, restored `*.png`) on release
`7af448eded61a2fb4892ecf95af4fa775786ae9190b8fe123ee7e414cee238a8`.
Final release SHA-256:
`0bd144f6e68d213a22830ccf9f9457d5e20ad857e40a77249fa70847d73188ae`.
Native preedit/commit/autosave/one-step exact-source undo pass at 360×1000,
1280×1200, and 1920×1080 with 200% text zoom. Final artifacts are
`spec-html-ime-{360,1280,200}-final` under `layout-previews/`, with matching
`.conversion.json`, `-preedit.png`, `-typed.png` and restored `.png` files.
Narrow and 200% preedit/committed captures were visually inspected: formatted
text remains readable, the composing caret is within the chosen italic run,
and following disclosure content stays below it. This uses isolated Weston 14
headless GL, 100% display scale, bundled Fraunces/Spline Sans fonts and copied
synthetic fixture 10; no user document or physical-session input is involved.

After builds and editing captures finished, two sequential final-release 10 MiB
mixed-Markdown scroll runs passed at **109.3 / 109.3 FPS**, draw p99
**6.55 / 6.09 ms**. Reports are
`layout-previews/spec-html-ime-scroll-10m-{1,2}.json`, each recording the final
binary hash. Environment: isolated Weston 14 headless GL, 1728×1080 output,
166.7% display scale, 120 Hz, ten measured seconds, AMD RYZEN AI MAX+ PRO 395
with Radeon 8060S, rustc 1.98.0, workspace release profile. These satisfy the
existing >60 FPS / <16.67 ms draw-p99 regression gate; they do not prove physical
display performance, HTML-heavy performance or bounded visible-window planning.
The unrelated missing SVG includes in app image-cache tests (lines 764/780)
still prevent a full-workspace test-pass claim.

Still unverified: real CJK candidate UI, input-method-protocol cancellation,
AT-SPI, and cross-fragment selection. XKB dead-key compose is a real native input
route, not evidence for every IME. In particular, the pinned backend may insert
an unresolved dead key before dispatching Escape; editor-handler cancellation
tests do not prove that upstream XKB route cancels without insertion.

### Direct HTML selection and atomic first edit (earlier increment)

Compatible top-level fragments now retain an immutable semantic text map beside
their Blitz raster. Clicking selects a glyph position without changing source,
document revision, canonical selection, or undo history. Drag/Shift selection,
word selection, keyboard movement, and plain-text copy operate on those verified
text leaves. The first edit converts and mutates through one document-core
transaction. There is no independently editable DOM or second mutable text store.
Replace, delete, rich paste, inline formatting, link changes, and paragraph split
are compared against explicit conversion followed by the normal editing command.
One undo restores the exact original HTML, including its authored styling.

Selection and UTF-16 input offsets are local to the pending fragment. Caret
scrolling and popup placement use the matching rendered geometry, not those byte
offsets in the surrounding Markdown projection. Focus loss hides the caret and
subdues selection; outline navigation exits the temporary selection. Zoom and
source-identical remeasurement retain semantic positions. Changed source or an
unverifiable geometry map rejects/cancels this entry path rather than guessing.
Invalid ranges and collapsed empty replacements do not convert or edit anything.

Native 200% testing caught a whitespace hit-test defect: clicking a gap between
HTML words fell through to the preceding Markdown heading. The retained map now
snaps blank areas inside a compatible fragment to the nearest verified glyph edge.
Clicks outside the fragment remain outside it. The dedicated whitespace test was
observed failing before this fix; native evidence uses the same gap coordinates.

Known limits remain explicit: pending selection copies semantic plain text, not
rich clipboard metadata; selection does not yet extend outside its HTML fragment.
At this earlier increment, direct IME required **Edit text** first; provisional
same-leaf composition is now covered above. HTML links/disclosures remain rendered rather than interactive,
and the full semantic descendant accessibility tree is not implemented. This is
not a claim that §13/§14 or the full HTML authoring requirement is complete.

Selection uses merged per-line highlight rectangles with a translucent accent
fill and crisp underline. Painting the ordinary light selection token over the
opaque raster only faded text on a matching authored green background; the
accent treatment retains a visible selection cue and readable glyphs.

Verification: **67 core + 145 view tests** pass. Focused warning-denied Clippy
passes for both crates/all targets and the application binary; application check,
formatting and diff whitespace checks pass. Crusty context `ctx_7bef20bc5226`
reports no new/worsened architectural findings. Full workspace packaging tests
are still outside this pass because of the unrelated deleted SVG asset reference.

Final release SHA-256:
`aba14645a185acec9e091d5ff226dea44153a764bd56b4e3f59faca3efdb0d3e`.
Native Wayland typing/autosave and **one** exact-source undo passed for fixture
10 at 360×1000, 1280×1200, and 1920×1080 with 200% text zoom. Reports and
visually inspected caret/typed captures are `spec-html-direct-360-final`,
`spec-html-direct-1280-final`, and `spec-html-direct-200-fixed` under
`layout-previews/` (`*.conversion.json`, `*-caret.png`, `*-typed.png`).
The last run uses the same `(1120, 781)` whitespace click that failed before
the hit-test fix; the earlier failure is retained in
`spec-html-direct-200-caret.png` with the misplaced heading caret. Native drag
selection was visually inspected in `spec-html-direct-selection-final.png`;
the matching `.selection.json` verifies exact clipboard text and unchanged
source. `spec-html-direct-selection.png` retains the earlier weak highlight.
`spec-html-explicit-final.conversion.json` verifies the existing explicit
conversion route and its separate typing/conversion undo entries.
These run on isolated Weston 14 headless GL at 100% display scale with the
bundled fonts, copied synthetic files, and a private Wayland input seat; the
user's physical session and documents are untouched.

Reproduce the previously failing native gap click:

```sh
python3 performance/capture-layout.py --fixture 10-html-fragments.md --width 1920 --height 1080 --zoom-steps 10 --convert-html-check 1120 781 --html-direct-edit --html-edit-within 'ordinary rich text' --output /tmp/tachyon-html-direct.png
```

After all builds and native edit captures finished, two sequential 10 MiB
mixed-Markdown scroll gates on this final release measured **107.1 / 107.4 FPS**,
draw p99 **8.38 / 8.14 ms**, at 1728×1080 output pixels, 166.7% display scale,
120 Hz isolated Weston. Both pass >60 FPS / <16.67 ms draw p99. Reports:
`layout-previews/spec-html-direct-final-scroll-10m-{1,2}.json`. These are
regression checks, not physical-display, HTML-heavy, or bounded-planner latency
claims. The earlier `spec-html-direct-scroll-10m-{1,2}.json` reports belong to the
pre-highlight release and are not the final binary evidence.

### Position-preserving HTML edit entry (earlier increment)

Compatible rendered HTML now has a local **Edit text here** context command.
Opening the menu does not change source or the canonical selection. Choosing
the command converts the fragment through the existing semantic converter and
places the canonical caret at the clicked glyph, rather than at the fragment's
beginning. The menu explicitly states that HTML styling becomes Markdown.
That increment provided only an editing entry point. The direct-selection
increment above adds selection and atomic first-edit conversion; the complete
semantic descendant tree remains open.

Blitz/Parley glyph-cluster bounds are extracted while the inert temporary DOM
exists and retained with the raster. Pointer/scroll handlers do not parse,
shape or paint HTML. Canonical text-leaf ordinals and UTF-8 offsets come from
the same converter/parser as the edit transaction. The complete non-whitespace
text sequences must match in source order before any hit targets are offered;
this disambiguates identical paragraphs without substring guessing. Authored
transforms, hidden-content mismatches, inline boxes and unavailable geometry
retain the existing explicit whole-fragment conversion fallback. The map is
bounded to 4,096 non-whitespace characters and 2,048 visited layout nodes.
Glyph midpoint hits are checked against Blitz itself. No new dependency or
external resource provider was introduced.

`ConvertHtmlToMarkdownAt` checks the expected original source, leaf index and
UTF-8 boundary atomically. Rejected/stale targets do not change revision or
source. Regression tests cover duplicate accented text, exact undo, styled
italic/link targets, padding, wrapping, hidden/transformed fallback, and view
coordinates after scrolling and 100/150/200% text scaling. The first native
check exposed a toolkit menu behavior: initial Down selected a leading label
instead of the command. Putting the actionable item first and the explanation
below it restored keyboard activation without patching the dependency.

All **62 core + 126 view tests** and focused warning-denied Clippy pass.
Native debug verification at 1280×1200 right-clicked inside `ordinary rich
text`, invoked the command with Down/Enter, verified insertion inside that
specific italic text, retained bold/italic/link source, then restored the entire
original fixture with two undo operations. Evidence:
`layout-previews/spec-html-edit-here-debug.conversion.json`, debug SHA
`beb514a40ed7a5e579c2623c1111e48c2932fa73ca1895bede4d80b9efd538bc`.
Final release SHA
`52a176178d23a179cb841fe41f7e8646085a8ae0b93cecb180a6667e55d9ad60`
passes the same native interaction at **1280×1200 / 100% text** and
**768×1000 / 150% text**, isolated Weston/Wayland, 100% display scale and the
bundled fonts. See `spec-html-edit-here-release.conversion.json` and
`spec-html-edit-here-150pct.conversion.json` under `layout-previews/`; their
`*-menu.png`, `*-typed.png` and final PNGs show explicit conversion, correct
caret/formatting and restored HTML. The retained map is bound to its exact
render-source bytes as well as the transaction's expected-source check.
Crusty validation (`ctx_31369fd0eb8f`, `task_81ef171c8aee5fc3`) reports no new or
worsened architecture findings. These are targeted editing checks, not proof of
full HTML selection, AT-SPI, IME, or the complete layout acceptance matrix.

The same final release passes the existing isolated **10 MiB scrolling gate**
at 1728×1080 output pixels, 166.7% display scale and 120 Hz: **106.6 FPS,
8.88 ms draw p99** (`layout-previews/spec-html-edit-here-scroll-10m.json`).
The native editing captures and release build had finished before that run.
This is the existing mixed-Markdown scroll workload, not a claim about cold
HTML layout latency, HTML-heavy memory use, or the user's physical display.

### Focus-aware editing locks

Background reflow now receives per-view editing focus. The containing source-order
row is a hard planner constraint, not a hysteresis bonus. Its measured widths
remain fixed when the window grows or text zoom creates additional logical space;
natural text height growth does not trigger a column switch. When retained widths
do not fit, the planner rejects alternative multi-column arrangements and uses
source-order stacks. Plain prose also keeps its previous measure while focused.
Internal list grids preserve their columns/widths across incremental refresh and
background reflow, including text exceeding the candidate nomination budget.

Row membership is rebound through canonical root IDs, not old numeric offsets.
Changed/interrupted root membership cannot inherit an unrelated lock. Focus
changes and native blur invalidate pending work; the next group becomes the lock
owner and the old group can be reconsidered. Selection and an active IME session
block background commits, including an empty marked range. Reflow captures the
visible caret's source offset where possible, otherwise the current reading
anchor, at commit time so an intervening scroll is not overwritten by a stale
captured position. These operations do not enter content undo history.

Bounded measurement may decline a grown locked node. Its retained candidate is
explicitly marked `height_estimated`; final native geometry still lays out all
canonical content. No cached old height is used to clip primary content. Full
structural-edit/IME/AT-SPI, focus-popover and resize acceptance remains open, as
does affected-window scheduling. This is progress on WP-06, not its completion.

The initial native test exposed an introduced title/intro overlap: different
retained measures had been mistaken for different columns. Column identity now
uses row/item ownership independently of width; a regression verifies unchanged
heading attachment and all subsequent vertical positions.

Synthetic fixture `14-edit-lock.md` verifies a 653-byte native paste into Beta,
an actual 90% text-zoom reflow, retained middle-column geometry and exact whole-file
undo. `layout-previews/spec-edit-lock-paste-wide-editing.png` was visually inspected;
`spec-edit-lock-paste-wide.edit.json` records debug SHA
`fcca18dc2afaccc6a27103921e860476fff710178fa4fffbe3630514499e5f7a`.
Earlier exact-marker checks failed on canonical Markdown punctuation escaping,
then a moved click target during the introduced overlap; those failed runs are
not acceptance evidence. The successful target uses an unpunctuated unique span.

`spec-edit-lock-blur-editing.png` and `spec-edit-lock-blur-blurred.png` were
visually inspected: the tall Beta card remains in its middle column while
focused and returns to source-order stack after the caret moves to the next
section. `spec-edit-lock-blur.edit.json` verifies the paste and exact undo on
debug SHA `88376d8e990f2de68186555c7df3e82335eb0227aa3813840d7fa21b3749b362`.

All **122 document-view tests**, formatting, diff-whitespace checks and focused
warning-denied Clippy pass. Coverage includes plain prose width locks, peer
growth, list refresh beyond the nomination budget, narrow fallback, root-ID
remapping, empty IME composition, and unchanged title/intro vertical gaps.
Crusty reports no new or worsened architecture findings.

Release SHA `5ce4594bfa93b782cc130cf929e8e3590b7fa100742eacf4de2ea6e172e1ac87`
passes the isolated Weston 10 MiB scrolling gate at 1728×1080, 166.7% display
scale and 120 Hz: **107.2 FPS, 10.50 ms draw p99**, over 10 measured seconds
(`spec-edit-lock-scroll-10m.json`). This is a scrolling pass, not an improvement
claim or a measurement of bounded planner latency. A separate project's Cargo
test was observed on the host near this run, so exclusive machine load is not
claimed. The full-workspace packaging-test issue recorded below was not changed.

The same final release also passes native paste/reflow/blur/undo:
`spec-edit-lock-release.edit.json`, with inspected active and blurred captures.
`spec-edit-lock-narrow-fallback.edit.json` verifies a native 130% text-zoom reflow
while Beta remains focused: all three sections stack, the caret stays in Beta,
and undo restores exact original bytes. The corresponding `*-editing.png` was
visually inspected. This tests reduced logical canvas width through real text
zoom, not a claim that every physical-window resize path has been exercised.

Commands used: `cargo test -p document-view --locked --lib --quiet`,
`cargo clippy -p document-view --locked --all-targets -- -D warnings`,
`cargo fmt --all -- --check`, `git diff --check`, and
`cargo build --release --locked --bin tachyon`. Native checks use
`performance/capture-layout.py` with fixture 14, `--edit-paste`,
`--edit-zoom-steps`, optional `--edit-blur`, and the isolated 10 MiB
`--generated-bytes 10485760 --perf-seconds 10` scrolling case.

### Inline math integration

`editor/inline_math.rs` measures real text-style formulas using the existing
bounded renderer/cache. Formula attachments wrap atomically with surrounding
prose; retained images, ascent/descent and original byte-indexed glyph geometry
are used for painting, selection and hit testing. Entering a math paragraph
reveals its canonical TeX for editing; other paragraphs remain rendered. Per-view
source-reveal state survives background reflow and restored selection. Retained
attachment ranges are local to their line so an earlier edit cannot corrupt them.

The native `spec-inline-fraction-edit.edit.json` records insertion inside
`\\frac{a+b}{c+d}` and exact undo. `spec-inline-math-copy.copy.json` verifies six
unique source markers in native clipboard order. Both record debug SHA
`f1354834ecd1fb08a1415f69d99565ed8b535b45361a1c4ca2edc84e1f152aa5` and predate the
focus-lock increment. Invalid, too-wide and RTL formulas retain complete source
fallbacks. Full formula accessibility, RTL layout and math-heavy stress evidence
remain open; the older display-math subsection below is historical evidence.

### Measured tables and native fit review

Table measurement now scans all supported headers/cells (at most 32 columns,
512 leaf segments, 64 KiB total and 4096 bytes per leaf). It uses native inline
font runs and Unicode word boundaries to derive minimum and preferred widths.
Unsupported or larger tables retain source-order estimated/overflow fallback,
not an allegedly measured partial sample. Unchanged canonical table Arcs reuse
constraints on edit refresh; a new font environment rebuilds them. Explicit
authored widths retain their existing behavior.

Eligible adjacent table groups and explanation/table pairs participate in the
bounded row search. Each column recursively positions its own cells and rows;
text wrapping, paint, hit testing and resize handles use the same fitted widths.
The table's full extent is recorded even when its trailing cells are offscreen.
Source order is unchanged; these are geometry-only arrangements.

Native review found two fit issues that NoopTextSystem alone did not reveal:

- An exactly sized semibold `Retries` header lost float precision through
  padding (`51.646` became `51.645996`), wrapping its final letter. Intrinsic
  constraints now round outward with a small guard for track/zoom round trips.
- The row reading-jump score ignored empty horizontal distance, rewarding a
  large gap after the compact table. It now uses vertical height and the empty
  trailing span plus the inter-group gap. Both tables retain their typography.

Both have failing-before/passing-after regressions. Native horizontal scrolling
also exposed an inverted sign between GPUI content deltas and our positive
hidden-content offset; the call-site regression now verifies forward/reverse
scrolling, and normal input reveals the trailing column in the 360px capture.

Verification: **61 core + 111 view tests**, focused warning-denied Clippy,
formatting and diff whitespace checks pass. Native isolated Weston captures:
`spec-tables-balanced-1920.png`, `spec-tables-balanced-360.png`,
`spec-tables-horizontal-fixed-360.png`, and `spec-tables-200pct.png`.
The 200% capture verifies enlarged stacked property-table layout, not every
offscreen table. The native desktop review kept compact intrinsic tables and
top-aligned headings rather than stretching cells to fill their assigned spans.

`spec-tables-cell-edit.edit.json` verifies typing in the first property cell,
autosave and exact whole-source restoration on undo (debug SHA
`a757355a10f91d4feb5a87ae836096dbea8ba5aa8206acc455d06d0de8b8866b`).
`spec-tables-copy-order.copy.json` verifies unique markers across both table
groups and the following section in real native clipboard order (final debug
SHA `b9def06a102efd9e4437cfa675fcb0439af97ba8176d946f8d78df731cc659f7`).
The harness waits for asynchronous clipboard publication on the private test
seat; the initial immediate reads returned `Nothing is copied` and were not
accepted as passes. Clipboard access never touches the user's compositor.

This advances AT-05/08/10/12, but does not complete their full acceptance matrix.
IME/AT-SPI, resize/anchor locking, measured galleries, bounded visible-window
scheduling, full HTML authoring and inline math remain required. Synthetic
fixture 12 is not the user's confidential Nudge document.

Final release SHA
`304922468dfa72b84440be78034d7177e8251703596994a3a283981e1a1d7d53`
passes the isolated Weston 10 MiB scrolling gate at 1728×1080 output pixels,
166.7% display scale and 120 Hz: **106.7 FPS, 8.38 ms draw p99**
(`layout-previews/spec-tables-scroll-10m.json`). This is steady-state native
rendering evidence, not a visible-window planner-duration or physical-display
claim. The same release passes native property-cell typing/autosave/exact undo
in `spec-tables-release-edit.edit.json`; `spec-tables-short-768.png` shows
the source-order stack in a short window. Crusty validation for
`ctx_47653f56a775` reports no new or worsened
architecture findings. A fresh whole-workspace test-build attempt still fails
at the two removed packaging-SVG includes in `markdown-app/src/image_cache.rs`;
the unrelated packaging changes were preserved.

### Bounded measured row-planner increment

`adaptive/rows.rs` now runs a backward dynamic program over contiguous windows
of at most 40 groups. Each row consumes the next 1–3 groups. The previous
template is explicit state, so transition penalties participate in the optimum.
Complete compact subsections can combine their own child groups, never adjacent
chapters. Major headings and thematic breaks delimit windows. Stack is always
available, including for opaque, oversized and unmeasured groups.

Candidates use exact 12 / 6+6 / 4+8 / 8+4 / 5+7 / 7+5 / 4+4+4 tracks.
Canonical-root partition, track widths, finite measurements, overflow and the
0.7-viewport-height bound are checked before placement. Decisions retain seven
normalized score terms, deterministic ties, 10% + 0.001 hysteresis and rejection
reasons. An invalid previous row cannot force stale geometry. A stacked
explanation/example has two internal flow rows for scoring. These remain
tunable policy defaults, not universal reading-preference claims.

Breakpoint-only peer sections were removed from initial preparation. They stay
stacked until native measurements exist. The adapter shares fonts, wraps,
presentation breaks, sizes and padding with paint. Prose preferred measure uses
up to 60 actual shaped graphemes, not an entire unwrapped paragraph: treating
the latter as the desired width incorrectly penalized normal prose wrapping.
Exact-width per-job results and existing native font caches are reused.

`LayoutSlot` carries a track start/span used by wrapping, paint and hit testing.
Major headings remain full-width above explanation/code pairs. Subsequent prose
starts below the taller column. Code Copy buttons and horizontal clipping are
local to the assigned column. Text-edit refresh retains current row slots;
full focused-row/IME/blur commit behavior is still not proven.

Verification: **61 core + 104 view tests**, doc tests, focused warning-denied
Clippy, formatting and diff whitespace checks pass. New tests cover lookahead
beating greedy pairing, 729 generated candidate sequences, hysteresis and
invalid-track escape, 123 canonical groups partitioned across four windows,
measured/rendered explanation-column height equality, unequal code widths,
short/narrow fallback, unchanged peer fonts and exact source preservation.
Adapter tests use GPUI NoopTextSystem; native evidence is separate.

Native isolated Weston/Wayland captures with bundled Fraunces/Spline fonts:
`layout-previews/spec-row-dp-{1920,360}.png`,
`spec-row-dp-product-1280.png`, `spec-row-dp-768-short.png`, and
`spec-row-dp-release-edit.png`. The final wide view shows three measured peer
sections and unequal explanation/code widths. Native code editing changed byte
335, autosaved, and undo restored the entire original fixture. The release
capture records SHA
`4730d29b03101a4b166d9ebc655081c7c5c4c4f5a824e6af9ac9a2252d993d87`.
Synthetic fixture 11 is not the user's Nudge document.

The same release passed a 10 MiB scrolling run at 1728×1080 output pixels,
166.7% display scaling and 120 Hz isolated Weston: **109.1 FPS, 5.64 ms draw
p99** (`layout-previews/spec-row-dp-scroll-10m.json`). Two small independent
captures overlapped startup. This is not physical-display or planner-duration
evidence. The DP window is bounded, but the current job still visits the whole
document; visible-plus-lookahead scheduling and timing instrumentation remain.

Debug SHA `089d9593def334077f6ca1c0e44d83a6978117926b7dfc5a65c23cb3f100dbd6`
also reached actual 200% text in `spec-blitz-zoom-settled-200pct.png`, with
reflowed HTML and complete rounded borders. `--reflow-wait 6` allowed cold
background reflow; it is not a latency measurement. The earlier one-second
`spec-blitz-zoom-verified-200pct.png` still has clipped retained geometry and is
**not** a completed-reflow pass. Avoiding that temporary clipping remains open.
Authored disclosure markers now use the bundled Blitz bullet font.

Remaining work includes table/image candidates, affected-window scheduling,
inspector/metrics, full focus/IME/RTL/AT-SPI acceptance, layout controls/conflict
resolution, inline math and complete HTML authoring. The whole-workspace check
still has the unrelated stale packaging SVG test paths described below. This
increment does not complete WP-03–07 or the thread goal.

### Blitz HTML integration increment (not complete HTML authoring)

Actual pinned Blitz 0.3.0-beta.2 now parses, lays out and paints supported HTML
fragments into retained 2× raster previews inside the native document. It uses
Blitz DOM/HTML/paint and anyrender's Vello CPU backend, not a webview or a
replacement editor. Original source remains canonical. The viewport and paint
scale agree; the native body font and its bold/italic variants are embedded.
Blitz's bundled bullet font supplies disclosure markers. The explicit
`DummyNetProvider` denies network, file and data resource requests; no shell,
navigation, HTML-subdocument or script execution provider is installed.

`document-core/src/html.rs` owns the bounded inert-fragment adapter. Scripts,
stylesheets and embedded documents are dropped from render input under the
existing inert policy; unsupported media, namespaces and widgets keep the
source fallback. Inline styles remain local to the fragment. Source input is
limited to 32 KiB, accepted traversal to 512 nodes/depth 32, and raster output
to 4 megapixels, maximum 1920×2048 logical px. These limits are not a proof of
a CPU-time bound for all authored CSS. The cross-document cache holds at most
8 entries / 16 MiB of compressed images; published visual lines retain their
own images, so cache eviction cannot trigger CSS/layout/raster work on scroll.

A native Copy HTML action retains exact source; Copy text provides decoded
text. Compatible top-level fragments expose explicit **Edit text** conversion
to canonical editable Markdown, as one undoable transaction. Bold/italic/link
semantics survive; authored CSS is intentionally replaced. Unknown structures
and details are not offered this conversion, because the existing converter
cannot preserve their complete semantics. This is not yet per-text-node
editing inside a retained styled HTML container.

Native synthetic fixture `10-html-fragments.md` was inspected at 1280×1200
and 360×900 (`layout-previews/spec-blitz-final-{1280,360}.png`). The native
conversion capture at 1280 clicked Edit text, verified formatting/link targets,
typed into the new text, and used two Ctrl+Z steps to restore the original
HTML byte-for-byte. JSON records debug binary
`1a0581e6ceda984c9a7144a34eb3b84cfa465ad5d66fa932ad5b5592e5ff94f3`.
Later marker/zoom refinements require final-build recapture.

Pixel regressions caught and then verified fixes for half-sized 2× text and
clipped bottom borders: the root causes were inconsistent viewport/paint scale
and default stylesheet ordering that reinstated the body margin. Real renderer
tests also cover width-dependent wrapping, authored open/closed measurements
and oversized fallback. Native inspection identified a rapid-zoom responsiveness
gap; zoom now scales retained geometry/resources immediately and requests the
existing generation-checked background reflow instead of synchronously
rerendering HTML. A regression verifies retained image identity and source/
selection preservation on the input path. The preliminary file named
`spec-blitz-200pct.png` only reached 110% and is **not** valid 200% evidence.

Remaining HTML work: native link hit targets using existing navigation policy,
interactive disclosure state/reveal behavior, per-text-node selection/editing
without converting the entire fragment, complete semantic descendants/AT-SPI
traversal, resource integration through the existing image owner, high-contrast
and full text-scale acceptance, bounded cold-render scheduling and long-document
performance. Current accessibility exposes decoded text as a paragraph, not
a complete equivalent HTML tree; the read-only preview is not accepted as the
final accessible editor.

The dependency graph reproduced 32 Stylo E0282/E0283 errors when GPUI logging
enabled `serde_fmt`. A narrow vendored `stylo_derive` 0.20.0 patch types the
macro's generated `Ok` error as `std::fmt::Error`, matching
[upstream issue/PR 452](https://github.com/servo/stylo/pull/452), without importing
the branch's unrelated changes. MPL source headers, license and patch provenance
are retained. The combined dependency graph then compiled.

### Display math and matrix correction increment (not complete math support)

`document-view/src/math.rs` uses pinned `latex-rust` 1.0.2 with STIX Two Math
to produce self-contained SVG paths. The canonical literal source remains
visible/selectable/editable below its preview. Malformed/unsupported input
retains source. Input/output/cache limits from the initial increment remain:
2048 source bytes, 256 tokens, 24 explicit nested groups, 4096×1024 logical px,
512 KiB SVG, 64 cached successes/failures. No external TeX process or network.

Standard standalone `$$…$$` now imports as an editable literal block with
`CodeBlockSyntax::DisplayMath`; the serializer retains double-dollar syntax.
Unchanged documents and undo retain exact original bytes. If an edit inserts
a conflicting double-dollar delimiter, serialization uses a math fence rather
than emitting broken delimiters (reopening includes the required fence newline).
Inline `$…$` and embedded double-dollar runs retain typed math-source spans,
but **inline formulas are still displayed as TeX text**, not native math.

The pinned vendored engine now centers matrix rows and both delimiters on the
math axis before enclosing fractions/scripts are laid out, and its SVG backend
applies the measured glyph scale. The earlier XML/SVG postprocessing adapter
has been removed. Real parser/layout/render regressions cover six matrix
delimiter styles standalone, inside fractions and inside superscripts (18
cases), plus 70% superscript glyph scaling. The 24 upstream internal math-engine
tests pass. Very large delimiter assemblies and broader TeX coverage remain
unverified.

Native `09-display-math.md` release capture at 1280×1200:
`layout-previews/spec-display-math-release-1280.png`. The matrix is visually
centered, the quadratic formula source was edited through autosave (first
changed byte 141), and native undo restored all original bytes. The evidence
records release SHA
`215266df2bf874622bf984b8f12a63eb17f9011d8a0ee9e7700d120ae13d160a`,
which predates this HTML increment. It must not be used as new HTML performance
evidence. Inline native layout, source-edit presentation, keyboard overflow,
AT-SPI and formula-rich stress verification remain open.

Verification during this increment: 61 core + 98 view tests and 24 vendored
math tests passed; focused warning-denied core/view Clippy passed. Additional
zoom regression/final checks are recorded below when complete. Full
`scripts/check.sh` stopped in app tests because the separately edited packaging
icon moved from SVG to PNG while two image-cache tests still include the old
SVG pathname. Those packaging changes were preserved; this is not a full
workspace pass.

### Measured internal-list selection

The native `build_measured_adaptive_plan` path now refines each nominated flat
list using stack/two-column/three-column measurements. The renderer and
measurement adapter share bold-prefix and explicit-arrow presentation breaks,
font runs, body leading, marker insets and card padding. Each list contributes
at most nine paragraph nodes at three exact widths; paragraphs over 4 KiB are
rejected before shaping. This bounds an individual list decision, **not** a
whole-document planning job.

Automatic grids reject more than five rendered lines per item, greater than
1.6× rendered item-height imbalance, actual text overflow, insufficient text
measure, or unavailable native measurements. Three–nine items nominate grids;
task/nested/multi-paragraph lists and explicit ordered cross-step references
remain vertical. Existing peer sections retain ownership of their contained
lists, rather than producing nested card grids.

Each candidate retains measured widths, heights, line counts, normalized score
terms and rejection reasons. Costs sum normalized **internal row** penalties;
the stack has one row per item. Useful-width penalties use actual intrinsic
text width, not character counts. The change term and 10% + 0.001 hysteresis
apply only to a prior measured decision, not the initial estimated nomination.
Invalid prior candidates immediately release their preference. Existing edit
refresh keeps prior arrangements; this does not yet prove the complete focused
row/resize/IME commit contract.

The font-free initial preparation still uses the old estimated nominations;
the measured background pass replaces those decisions. Removing that remaining
estimated-layout path belongs to the full planner/fallback integration.

New measurement-adapter regression tests exercise wide/medium/narrow list choices,
rendered-vs-measured line count and height equality, exact source-order text
coverage, source bytes, nested/cross-reference/ten-item fallback, and glyph-width
cache separation. These use GPUI's deterministic **NoopTextSystem**, not actual
font glyph metrics. They prove adapter behavior and cache isolation; native
Wayland captures are required separately to verify actual fonts. The grid-edit
test uses six genuinely short independent
labels, so it tests editing a legal measured grid rather than assuming the old
character-count decision. `06-measured-list-fit.md` is an explicit synthetic
fixture for runtime validation; it is not the user's Nudge document.

The first native capture exposed a startup hysteresis defect: a measured plan
could be prepared at the provisional 760 px width before the first real canvas
bounds arrived, causing two columns or a stack to persist at 1920 px. Measurement
now starts only after the renderer publishes the first actual bounds. The
`provisional_canvas_does_not_seed_measured_layout_hysteresis` regression checks
that no measured job starts before those bounds exist.

Subsequent native pointer/edit captures exposed two interacting faults, now
fixed: a Wayland width configured between render and paint did not request
another render, and the stack candidate counted its intentional outside prose
margin as intrinsic content width. This could publish a worse stack only after
unrelated pointer activity. Paint now notifies only when the canvas width
actually changes; stack scoring measures the real bounded prose width.
The 1280 px measurement-adapter regression failed before the scoring fix and
passed afterwards. An explicit native six-card surface probe also failed on
release `70df8984…` and passed on the corrected debug `7b0679e2…` through native
selection, typing/autosave and exact-byte undo. These probes verify specific
card geometry; they do not replace semantic traversal or IME acceptance.

`capture-layout.py` now accepts `--binary`, `--log-output`, `--layout-probes`
and `--probe-color`. The expected color prevents an already-missing layout
from passing a before/after comparison. Binary identity is captured from the
running process rather than a pathname that Cargo could replace mid-test.
Temporary diagnostic logging was removed from the Rust renderer.

### Previous integration baseline

- Canonical group tests cover preamble, skipped levels, duplicate heading IDs,
  typing/undo anchor stability, nested list ownership, opaque HTML and thematic
  barriers. 1,296 generated four-block sequences form deterministic exact
  partitions. This is group analysis evidence, not AT-20 planner completion.
- Thematic breaks now have a real source-order rule/semantic geometry instead
  of disappearing from projection. Two images separated by a break cannot pair.
- Pairwise gaps: heading attachment 10 px, adjacent explanation/content 12 px,
  paragraph/group gaps 24 px, subsection lead-in 32 px, major-section lead-in
  44 px. Table vertical cell padding is 10 px, kept separate from outer gaps.
- Card layouts preserve the design-system heading/body sizes; no 16 px body
  reduction to make cards fit.
- Native spacing captures (before measured wrapping) are under
  `layout-previews/spec-spacing-{360,768,1280,1920}.png`, 100% text and display
  scale, isolated Weston/Wayland. The 360 px capture exposed avoidable mid-word
  title wrapping, which motivated native font measurement.
- Font-wrap tests verify exact source-range coverage, extended grapheme
  boundaries, exact-width cache keys, and a word fitting at its actual shaped
  width remaining unbroken.
- `scripts/check.sh` passes 177 tests (55 core, 82 view, 1 fixture, 39 app),
  formatting, workspace checks, warning-denied Clippy and documentation tests.
  Crusty validation reports no new/worsened architecture findings.
- The release build with SHA-256
  `4bd4f2bffc096e84349c25c2cdd66b56fd3ec7162564cc1dc28f1877cac0f944`
  was visually inspected at 360×900, 768×600, 1280×1000 and 1920×1080,
  plus 1920×1080 with native 200% **text zoom**. Files are
  `layout-previews/spec-measured-{360,768-short,1280,1920,200pct}.png`.
  All use isolated Weston 14 headless GL, Wayland, 100% display scaling and
  the bundled font set above. Narrow title words now fit according to their
  actual font metrics; source paragraphs use the available measure.
- Native typing/autosave/undo passed in the title at narrow width and 200%
  text, and in the first journey card at 1920×1080. The `*.edit.json` records
  confirm source changes reached disk and undo restored the original bytes.
  `spec-measured-card-edit.png` retains the three-column card row after undo.
  This is not evidence for paste/IME, screen-reader traversal, or the entire
  AT-10 matrix; those remain open.
- Two 10 MiB scrolling runs at 1728×1080 output pixels, 166.7% display scale,
  120 Hz isolated Weston measured **109.2 / 108.6 FPS**, with draw p99
  **5.62 / 8.51 ms**. Both passed the existing >60 FPS / <16.67 ms draw-p99
  gate. Reports: `spec-measured-scroll-10m-{1,2}.json`. Run 2 overlapped a
  small separate fixture-edit capture during startup; run 1 had no other
  capture running. These are isolated-compositor scrolling measurements,
  not physical-display or bounded-planner timing claims. The full-document
  reflow path still needs replacement with bounded measured planning.

Reproduction examples:

```sh
python3 performance/capture-layout.py --fixture 05-product-specification.md --width 360 --height 900 --edit-check --output /tmp/tachyon-narrow.png
python3 performance/capture-layout.py --fixture 05-product-specification.md --width 1920 --height 1080 --zoom-steps 10 --edit-check --output /tmp/tachyon-text-200.png
python3 performance/capture-layout.py --fixture 05-product-specification.md --width 1920 --height 1080 --select 600 980 600 980 --edit-check --output /tmp/tachyon-card-edit.png
python3 performance/capture-layout.py --width 1728 --height 1080 --scale 200 --generated-bytes 10485760 --perf-seconds 10 --output /tmp/tachyon-scroll.png
```

The harness copies synthetic fixtures into a temporary workspace before editing;
it never types into the user's document or unlocks/operates the physical session.

### Editorial composition increment

- `47-editorial-composition.md` exercises a title and lead, section
  introduction plus independent task list, repeated heading/table units,
  repeated heading/prose units, explanation plus code, and a return to ordinary
  prose. It contains no layout directives or duplicated presentation content.
- A section heading and its introduction can now remain full width while a
  following compact independent unordered/task list shares a measured two-track
  row with the prose. Ordered and nested lists are excluded, narrow windows
  retain the source-order stack, and focused editing retains the chosen card
  geometry.
- Three legal matched sibling sections are one coherent peer unit. A complete
  measured three-card row wins when readable; if that row fails width, height,
  or overflow gates, two-card candidates inside the matched trio are rejected
  so the result is a full source-order stack rather than an orphan plus pair.
  After a narrower viewport forced that stack, a wider canvas may reform the
  complete row without paying the ordinary arbitrary-layout change penalty.
- Native 1280×1920 captures are
  `layout-previews/editorial-before-1280.png` and
  `layout-previews/editorial-after-1280.png`. The latter visibly retains all
  three architecture siblings in one card row and composes the checklist
  beside its introduction. Its source-unchanged check passed with SHA-256
  `ce533a00231b29f813d86b4637af5bade9bf58cfddbaf7047c2e21180b05235a`.
  The optimized binary SHA-256 is
  `4c8fdecef9430aae7469ec03c03a37d6ecb6609d9aeaf7e508f9fa7c75afa39f`;
  cold-to-warm planner passes were 37.4 ms, 8.7 ms, 0.7 ms, and 0.6 ms.
- The same optimized binary passed a 10 MiB continuous-scroll run at 1920×1080
  and 120 Hz with **106.6 FPS** and **8.03 ms draw p99**, while retaining exact
  source bytes. Report:
  `layout-previews/editorial-composition-scroll.json`. The first attempt with a
  private AT-SPI bus hit a Wayland transport buffer limit before producing a
  performance report; the successful performance run therefore did not claim
  simultaneous AT-SPI load.
- Native select-all/copy traverses the introduction/list row, all table groups,
  the three-card peer row, and the explanation/code row in canonical order.
  The complete 2,131-byte semantic clipboard equals the reviewed
  `47-editorial-composition.copy.txt` golden exactly, including tab-separated
  table cells, every list item and paragraph, code newlines, and the final text.
  Its SHA-256 is
  `eba4fc18c6c090b5e33b716eb599d91487d87fe5784f0acd51da5267691ca3e8`;
  ten cross-composition markers additionally occur exactly once in source
  order, and the layout operation leaves the fixture byte-identical. Evidence:
  `layout-previews/editorial-copy-exact-1280.copy.json`, `.source.json`, and
  `.png`.
- Native dead-key IME and plain-text paste now pass inside the middle card of
  the three-column peer row. During preedit the private AT-SPI tree contains the
  provisional accent in the existing paragraph node while the source remains
  unchanged; commit/autosave targets that paragraph, 1.5 seconds of focused
  idle layout retains the row, and one content undo restores every original
  byte. The paste path has the same target, idle and exact-undo oracles.
  Evidence: `layout-previews/editorial-ime-card-1280*` and
  `layout-previews/editorial-paste-card-1280*`. The harness now distinguishes a
  standalone preedit node from one provisional Unicode codepoint inserted into
  an existing accessible label, and compares regenerated plain-text nodes after
  projecting only legal Markdown punctuation escapes.
- A private-session AT-SPI traversal of the same 1280×1920 composition publishes
  190 nodes with all 12 authored headings in order, five task checkboxes under
  their canonical list, and exact 5×3, 5×2, and 4×2 table shapes. The three peer
  headings share y=1401 and retain source-order x positions 260, 596, and 932;
  their paragraph children, the following explanation/code pair, and final
  heading also traverse in canonical order. The document editor exposes native
  focus, code text is accessible, and source remains byte-identical. Evidence:
  `layout-previews/editorial-atspi-1280.atspi.json`, `.source.json`, and `.png`.
  The structural oracle has rejecting regressions for reordered peer content,
  broken visual rows, and incomplete table-cell hierarchy.
- AT-11 now has native wide→narrow→wide evidence under Weston 14's desktop
  shell, which permits real client resizes rather than kiosk-shell no-ops. The
  validation-only binary resized the window from 1360 to 720 logical pixels;
  the exposed editor width changed 1030→622→1030 pixels. While reading, the
  `3. Architecture and persistence` anchor stayed exactly 12 pixels below the
  viewport top through narrow reflow, a repeated identical resize, and wide
  restoration. The peer group changed from one three-card row to three complete
  stacked sections and back to one row; all heading identities and visible
  geometry remained stable and the source stayed byte-identical. Evidence:
  `layout-previews/editorial-resize-reading.resize.json` and the
  `editorial-resize-reading-reading-{wide,narrow,restored-wide}.png` captures.
- The companion active-edit run used native Find, selection collapse, typing,
  autosave, resize, and undo. The exact canonical caret remained focused at
  `1479..1479` before and after reflow, the inserted `x` remained inside the
  intended middle-peer paragraph, the trio used the legal narrow stack, and one
  undo restored every original byte. Evidence:
  `layout-previews/editorial-resize-editing.resize.json` and
  `editorial-resize-editing-editing-{typed-wide,narrow}.png`. F8/F9/F10 state
  and resize commands exist only behind `layout-validation`; the production UI
  remains automatic-only. Both runs used optimized binary SHA-256
  `a2a16a2f0a315b8a94302ea8f1c8cb72df734eb0eec611f259070fa99fa278f6`.

### Typography-environment remeasurement and wrapped-line anchors

- `48-typography-direction-overflow.md` is a bounded synthetic body-font,
  200%-text, Arabic/Hebrew and long-unbroken-content fixture. F6/F7 switch the
  component theme between bundled `Noto Sans Tachyon` and
  `Spline Sans Tachyon` only in a `layout-validation` build; no font or layout
  control is exposed in the production UI. A font mismatch creates a new
  immutable `FontMeasurement`, clears the table lock, advances geometry and
  runs a committed measured plan rather than reusing a previous font's cache.
- The native 100% run records a real 29 px downstream geometry change under the
  alternate face, 132 shaping calls and 27 wrap-cache misses, then exact
  geometry restoration. The 200% run legally retains the same wrap boundaries
  but records 128 shaping calls and 22 wrap-cache misses for both alternate and
  restored environments. All six heading identities and every source byte stay
  unchanged. Evidence: `layout-previews/typography-font-reflow-{100,200}.font.json`
  and their `-{before,alternate,restored}.png` captures.
- The first two 200% runs reproducibly moved the chosen heading
  `69→126→184` pixels down the viewport. The renderer's semantic anchor stored
  the visible wrapped line's start, but resolution treated both the preceding
  line's end and following line's start as inclusive and selected the earlier
  iterator entry. `resolve_scroll_anchor` now prefers an exact line start
  before its inclusive mid-line/terminal fallback. A focused regression locks
  that boundary rule while the existing insertion-above test remains green.
  With optimized validation binary SHA-256
  `1f9c3ea44a5141d65c50b9c864cb45ce2c419713e689e87596e6eabc856708ab`,
  both native runs keep the heading at exactly 11 px through font change,
  repeated identical environment and restoration.
- The reviewed 100% screenshots show the Arabic and Hebrew paragraphs painted
  in native visual RTL order, and the long inline identifier wraps without
  widening/clipping the page. Existing table/code/HTML tests prove retained
  local horizontal overflow for technical blocks, including 200% text. This is
  partial AT-12 evidence: native RTL caret/hit-test order and an actual delayed
  asynchronous font-load event still need independent oracles before AT-12/13
  can be closed.

### Native content-sensitive list, row, and table acceptance

- `adaptive_layout_check.py` now inspects the complete private-session AT-SPI
  tree and window-relative geometry for fixtures 06, 11, and 12. It rejects
  missing/duplicated/reordered content, split or missing list ancestry,
  overlapping/ambiguous visual order, malformed table row/cell hierarchy, and
  a layout that contradicts the declared wide/medium/narrow profile. Every run
  also hashes and re-reads the isolated fixture source and retains a screenshot.
- At **1440×1200**, the two short six-item groups form source-ordered 3×2 grids.
  At **900×1200**, the one-word directions form a 2×3 grid while the longer
  labeled descriptions stack because their measured content no longer fits
  comfortably. At **480×1200**, both stack. The uneven, cross-referencing, and
  ten-item lists remain vertical at every width; nested paragraphs and code
  retain their authored traversal. Ordered numbers remain visible in the
  reviewed captures, while all item paragraphs retain one canonical native
  list-item/list ancestry. Evidence:
  `layout-previews/adaptive-lists-{wide,medium,narrow}.{adaptive.json,png}`.
- At **1280×1100**, the three repeated review sections form one complete
  source-ordered card row. The long worker example remains a readable stack,
  while the shorter second explanation and code use two non-overlapping
  columns. At **900×1100** and **480×1200**, all of those groups become complete
  stacks; no orphaned pair is retained, and the thematic break keeps the final
  section outside the preceding group. Evidence:
  `layout-previews/adaptive-rows-{wide,medium,narrow}.{adaptive.json,png}`.
- At **1280×1100**, the 161 px runtime table and 518 px comparison table share
  one source-ordered row without being flattened to equal widths. At **900×1100**
  and **480×1200** they stack. All four tables preserve their exact authored
  header/cell names and 4×2, 4×3, 4×3, and 3×2 native structures; the narrow
  long-identifier table keeps its bounded horizontal overflow. Evidence:
  `layout-previews/adaptive-tables-{wide,medium,narrow}.{adaptive.json,png}`.
- All nine native runs use optimized validation binary SHA-256
  `1f9c3ea44a5141d65c50b9c864cb45ce2c419713e689e87596e6eabc856708ab`,
  isolated Weston 14/Wayland and private accessibility/session state at 100%
  display/text scale. Six oracle regressions cover legal wide/narrow cases plus
  reordered list content, broken list ancestry, overlapping peer cards, and a
  missing table cell. This supplies the requested AT-01–05 width, fit,
  fallback, source-order, hierarchy, and intrinsic-table evidence; it does not
  close the unrelated remaining acceptance rows below.

### Native chapter barriers and duplicate identity

- `49-chapter-identity.md` contains two unrelated long sibling chapters with
  identical H2/H3 labels and repeated prose. The second repeated paragraph has
  one unique lowercase edit target, but the surrounding wording remains
  deliberately ambiguous. No layout directive or presentation-only duplicate
  is embedded in the fixture.
- The private AT-SPI oracle verifies exact seven-heading source traversal,
  exactly two distinct persistent identities for each repeated heading level,
  and a hard chapter barrier: every semantic node owned by chapter one ends
  before chapter two begins. It then uses native Find, collapses the selected
  target, types `x`, waits for autosave/reflow, and proves all six duplicate
  heading identities plus the targeted paragraph identity are unchanged. The
  complete first-chapter byte prefix is identical during the edit; one content
  undo restores the original file byte-for-byte.
- Optimized validation binary SHA-256
  `1f9c3ea44a5141d65c50b9c864cb45ce2c419713e689e87596e6eabc856708ab`
  passes at **1280×1000** and **480×1100**, 100% display/text scale, isolated
  Weston 14/Wayland and a private accessibility/session bus. In both widths the
  first chapter's final semantic bottom and second chapter top retain a 45 px
  gap before, during, and after the edit. Reviewed captures show the caret only
  in `beta edit targetx` and the scrollbar on the outer window edge. Evidence:
  `layout-previews/chapter-identity-{wide,narrow}.identity.json` and their
  `-{before,edited,restored}.png` captures.
- Five rejecting/passing oracle regressions cover a legal transition, chapter
  overlap, duplicate native paths, a mutation leaking into chapter one, and a
  heading identity change. Existing group-analysis tests independently retain
  duplicate canonical anchors across ordinary typing. Together these provide
  native AT-06/07 barrier, identity, targeted-edit, and exact-undo evidence;
  production remains automatic-only, so no obsolete user override is invented.

### Exhaustive feasible row-template search

- `exhaustive_small_sequences_are_deterministic_legal_exact_partitions` now
  exercises all seven 12-track templates: stack, equal pair, four unequal pair
  orientations, and the three-column row. It enumerates all 729 mixed
  pair/triple rejection masks at a 1280 px canvas for each preferred adaptive
  template, plus all-available, boundary-rejected, and all-rejected cases at
  360, 768, and 1920 px. This is 4,428 generated candidate sets, and every set
  is solved twice.
- Every result must cover canonical groups `0..6` exactly once and in order,
  contain only `RowCandidate::legal` rows, reproduce the identical partition
  and template sequence on the second solve, and match every selected width to
  the exact feasible 12-track span at that canvas. The test also requires that
  all seven templates win at least once; a template silently excluded by
  candidate order cannot pass.
- The independent group analyzer still checks 1,296 generated Markdown block
  sequences for exact deterministic ownership, while the native AT-01–05
  fixtures above exercise the corresponding stack, equal/unequal pair, and
  three-column renderer paths. The strengthened focused planner test completes
  in about 2.8 seconds after compilation on this machine. This closes AT-20's
  bounded combinatorial evidence without changing production scoring or
  geometry.

### Native inert HTML side-effect boundary

- `50-inert-html-effects.md` combines a safe authored disclosure with eight
  loopback references across script, iframe, stylesheet, image, video poster,
  video source, CSS background, and unknown custom-element markup. The normal
  HTML safety policy remains unchanged: safe details use the Blitz renderer;
  active/unsupported markup is inert visible source/fallback rather than being
  executed, fetched, or silently converted.
- The capture harness binds an ephemeral **loopback-only** HTTP server before
  launching the app and substitutes its port only in the temporary fixture
  copy. A control GET must reach the monitor and is then cleared, so an empty
  result cannot come from a broken listener. During app startup, three explicit
  detailed replans, native AT-SPI open, and native AT-SPI close, the monitor
  records **zero requests** at all three checkpoints.
- The safe disclosure retains one native identity, Click action, expandable /
  expanded state, correct closed/open body visibility and byte-identical source
  through both transitions. Content following all unsafe fragments remains
  accessible, and the reviewed opened capture shows conservative fallbacks and
  literal preserved markup without remote content. The detailed trace contains
  eight reports, seven committed measured plans and all three requested warm
  replans. Evidence: `layout-previews/inert-html-effects.effects.json`,
  `.planning.json`, and `-{initial,opened,restored}.png`.
- The run uses optimized validation binary SHA-256
  `1f9c3ea44a5141d65c50b9c864cb45ce2c419713e689e87596e6eabc856708ab`
  at **1280×1000**, 100% display/text scale, isolated Weston 14/Wayland and a
  private accessibility/session bus. Five pure oracle regressions reject any
  observed request, an unverified monitor, disclosure identity replacement,
  lost conservative fallback, or source/state failure. This supplies AT-14's
  safe-renderer, authored-state, inert-measurement, and no-external-effect
  evidence without enabling a network provider in the app.

### Automatic-only intent and native boundary hysteresis

- The later product direction supersedes AT-16's proposed Plain / Adaptive /
  Focus chooser and per-group overrides: production has one automatic layout
  intent and stores no layout-mode preference. The feature-gated validation
  build adds only keyboard-driven window resize commands; the native AT-SPI
  tree contains no layout, Plain, Adaptive, or Focus control at any sampled
  width. A fresh wide process independently chooses the measured three-card
  row, so process reopen cannot restore a hidden manual layout mode.
- `boundary_layout_check.py` reveals the real chapter first, then changes the
  native window by 8 logical px per step. In the retained native run the peer
  group stayed a legal three-card row through **806 px** of editor width and
  changed directly to the legal source-order stack at **798 px**. There was no
  invalid intermediate geometry. The oracle then alternated those adjacent
  widths four times; every sample retained the stack rather than flapping back
  and forth. Expansion to 1030 px retained legal geometry, as permitted by the
  previous-legal-candidate tie rule.
- All heading AT-SPI identities remain exact through 38 geometry samples. A
  native Ctrl+Z after the layout-only sequence is a no-op and the copied fixture
  remains byte-identical, proving that automatic composition does not enter the
  content undo history. Existing reading/editing resize runs separately prove
  anchor/caret preservation and exact content undo when a real edit precedes
  reflow.
- Evidence: `layout-previews/automatic-boundary.boundary.json`, `.planning.json`,
  `.log`, and `-wide.png`, `-first-stack.png`, `-restored-wide.png`. The run used
  optimized validation binary SHA-256
  `d8aa955741f47982c5111d8fc70cbe7255c73a3dcfe939bfef85477314de1bc1`,
  isolated Weston 14/Wayland at 120 Hz, 1440×1000 output, 100% display/text
  scale, and a private AT-SPI/session bus. All 40 detailed planner reports
  committed; worker durations were 0.319–27.404 ms. Five pure oracle tests also
  reject topology thrashing, source mutation, invented controls, or illegal
  transition geometry. This closes the effective automatic-only AT-16 contract
  and AT-17.

### Current-production long-document acceptance

- The user's later performance policy explicitly permits slower startup while
  requiring scrolling above 60 FPS. AT-18 therefore uses the existing eager
  canonical geometry build at open, but requires the steady scrolling and
  localized editing paths to remain bounded. This does **not** claim that cold
  whole-document startup reflow is viewport-bounded.
- The non-confidential fixture `17-long-layout-spec.md` is explicitly synthetic:
  22,967 whitespace-separated words, 148,655 UTF-8 bytes, exactly 20 numbered
  H2 sections, 60 dense tables, repeated six-item feature groups, and 362 roots /
  1,522 rendered segments. It contains no Nudge text. Its only H1 and its 20 H2
  headings are authored source; the layout system does not invent summaries,
  fields, status, or navigation headings.
- Current production binary SHA-256
  `b473c707192ec449e0d33cba11312863f4b64df336594a77d44dc2fbe126dfc5`
  passes a ten-second continuous bidirectional native scroll run at 1728×1080,
  166.7% display scale and 120 Hz: **108.7 presented FPS**, 6.87 ms draw p99,
  10.84 ms input-latency p99, zero ≥25 ms stalls/intervals, and 0.55% missed
  presentation deadlines. Source remains byte-identical. Paint continues to use
  the bounded viewport slice; no layout job is dispatched merely for traversing
  already prepared content.
- On the same production binary, a native pointer edit in the unique fixture
  disclosure paragraph reaches autosave, remains stable through one second of
  focused idle, and one Ctrl+Z restores all 148,655 source bytes exactly. The
  edit/undo committed workers take 3.25/1.70 ms, search only two visible/lookahead
  DP windows while deferring 19 chapters, reuse 17/18 group footprints and all
  18 list-item footprints, and publish retained geometry with zero anchor
  displacement. A separate native Select All/copy finds `S1-F12`, `S10-F12`,
  and `S20-F12` exactly once in source order at byte positions 7,178, 71,369,
  and 142,809, including unvisited offscreen chapters.
- Evidence: `layout-previews/current-long-spec-scroll.json` and `.log`,
  `current-long-spec-edit.edit.json`, `.planning.json`, `-typed.png`, `-idle.png`,
  and `current-long-spec-copy.copy.json` / `.source.json`. These current-binary
  results supplement the earlier twelve-sample geometry-cache and chapter-window
  evidence above. Under the explicitly revised startup policy, they close AT-18
  without weakening source-order, edit, or steady-state scrolling requirements.

### Current-production responsive visual matrix

- The retained production matrix uses binary SHA-256
  `b473c707192ec449e0d33cba11312863f4b64df336594a77d44dc2fbe126dfc5`,
  isolated Weston 14 headless GL/Wayland with kiosk shell at 120 Hz, 100% display
  scale, bundled **Spline Sans Tachyon**, **Spline Sans Mono Tachyon**, and
  **Fraunces Tachyon**, and `47-editorial-composition.md`. Every run verifies the
  copied fixture remains byte-identical.
- Reviewed captures at **360, 768, 1280, and 1920×900 logical px** show the same
  authored hierarchy and consistent content leading edges. At 360 the navigation
  is absent and prose/cards form one readable stack; at 768 navigation remains
  collapsed while the independent confirmation group pairs only when it fits;
  1280 restores Files/Outline and the paired opening; 1920 uses the wider canvas
  for the pair without stretching prose across the page. Ordered source flow and
  heading attachment remain clear in all four.
- The **1280×480** short-height capture keeps identical horizontal composition
  and clips only at the viewport edge for ordinary document scrolling. The
  **1280×900 / 200% document-text** capture remeasures and stacks the opening,
  keeps the outer document scrollbar at the window edge, preserves full heading
  and paragraph glyphs, and does not page-clip or ellipsize essential text.
- Artifacts: `layout-previews/visual-matrix-{360,768,1280,1920}.png`,
  `visual-matrix-short.png`, `visual-matrix-200pct.png`, their `.source.json`,
  `.planning.json`, and `.log` files. All six source oracles pass. This closes the
  required responsive visual review matrix; renderer-specific math/Blitz
  selection and native RTL/delayed-font cases remain tracked separately.

The earlier native panic/timeout recovery evidence satisfies AT-15's required
usable stack, stable primary nodes, late-result rejection, source-order copy,
edit/undo, and non-blank document outcomes. The standalone linked-gallery
evidence likewise satisfies AT-19's required complete images, source order,
authored target, accessible descriptions, wide/narrow fallback, activation and
unchanged source. Their documented limitations concern broader interactions
outside those acceptance fixtures, not a missing required outcome in AT-15 or
AT-19.

### Native direction, delayed fonts, and current renderer closure

- The validation build now omits **Noto Sans Tachyon** from startup and registers
  its two bundled faces only when the native F6 acceptance action runs after the
  document is visible. The first action records `loaded=true`; applying the same
  environment again records `loaded=false`, so the oracle distinguishes a real
  font-completion event from a warm theme toggle. Both 100% and 200% runs commit
  freshly shaped geometry, retain every heading identity, keep the Reading
  anchor exactly 11 px below the viewport edge through alternate/repeat/restore,
  restore the original geometry, and preserve all source bytes. Evidence:
  `typography-delayed-font-{100,200}.{font.json,log,png}`.
- Bidirectional paragraphs no longer consume GPUI wrap boundaries as though
  visual glyph order were monotonically increasing source order. A bounded
  logical wrapper shapes real source-order word/grapheme candidates, and the
  editor derives pointer/caret stops from shaped visual cluster positions.
  Native Arabic checks at 100% and 200% prove that right/left hits map to
  reversed logical offsets while Left/Right move physically left/right; all
  carets remain collapsed and source is unchanged. The inspected 200% capture
  wraps Arabic and Hebrew inside the canvas while the following unbroken Latin
  identifier remains scoped to its own content. Evidence:
  `typography-rtl-{100,200}.{rtl.json,log,png}`.
- Optimized validation binary SHA-256
  `75cd965e87775bda0fe05eed2165fe535f24b17f20a9219ab62934398732c22b`
  also passes the current Blitz HTML AT-SPI disclosure/text/source check at
  1280×1000 and the MathML/token/overflow-value/source check at 1280×1400 with
  200% document text. The formula's native value reaches its exact 1,223 px
  maximum. Evidence: `current-html-atspi-1280.*` and
  `current-math-atspi-200.*`. Current core/view tests additionally exercise
  HTML first-edit conversion and exact undo, rich preview selection, inert
  links/resources, malformed/oversized fallbacks, inline/display math editing,
  invalid formula fallback, keyboard overflow and exact source preservation.
- The complete current check set passes **439 Rust tests** (105 core, 294 view
  with two installed-system-font cases ignored, one fixture test and 39 app
  tests) plus **55 Python oracle tests**. Default and `layout-validation`
  warning-denied Clippy, all-target checks, formatting and diff whitespace are
  clean. No test-only controls are present in the normal release.
- Normal production binary SHA-256
  `409c4226486642b765154f6526de4bb1513582040cc64cd6ec781086dbe5e36c`
  passes the final 10 MiB continuous-input run at **109.1 FPS**, 5.28 ms draw
  p99 and 10.00 ms input-latency p99, with no missed presentation deadlines,
  no ≥25 ms application stalls/intervals, and the generated fixture hash
  retained in the report. Evidence: `current-rtl-long-scroll.json` and `.log`.

These results close AT-12 and AT-13 for the native Linux Wayland target and
complete the requested Blitz/math acceptance alongside the prior edit/undo,
safe-fallback and responsive matrix evidence. Cross-platform assistive
technology and physical-display profiling remain useful follow-up work, not
unmet outcomes in this target-specific contract.

## Renderer dependency investigation (not implementation evidence)

Context7 documents inert default net/navigation/shell providers in Blitz. The
registry currently resolves `blitz-html` to `0.3.0-beta.2` (MIT/Apache-2.0,
MSRV 1.89); the docs.rs `latest` landing page still shows 0.2.0, so integration
must use the downloaded matching source. `HtmlDocument::from_html` takes a
`DocumentConfig`, and exposes its `BaseDocument` through Deref/`into_inner`.
Painting, font setup, text hit boxes, details state, safe resource handling and
canonical edit mapping still need implementation and verification.

`latex-rust` 1.0.2 is now pinned for math fence previews (MIT/Apache-2.0,
MSRV 1.76), with embedded STIX Two Math and real SVG output; see the partial
integration and known renderer-quality issues above. Inline/display integration
and final selection acceptance remain open. Primary references: [Blitz](https://github.com/dioxuslabs/blitz),
[LaTeX-Rust API](https://docs.rs/latex-rust/1.0.2/latex_rust/).

## Acceptance status

All AT-01–19 cases and hard invariants INV-01–12 now have Linux Wayland evidence
in this ledger, including current-binary native oracles for the formerly open
AT-12/13 and renderer rows. §20 handoff artifacts are retained under
`performance/layout-previews/`; synthetic sources remain under
`performance/layout-fixtures/`. Newspaper columns, masonry, table record cards,
sidecars and model-generated semantic content remain outside MVP as the contract
specifies.

### Editorial decision-panel refinement

- A level-two section containing introductory prose, a short colon-terminated
  label, and three to nine flat unordered/task items now nominates a measured
  editorial split. The heading stays full width, prose occupies the leading
  column, and the authored label travels with the list inside one quiet panel.
  No text is synthesized and canonical source order is unchanged.
- The panel uses one rounded native quad with an integrated green leading rail;
  its short authored label receives semibold heading color. Neutral peer and
  list cards retain their existing treatment.
- Heading rhythm now reserves 30 px between an opening H1 and an immediately
  following H2, 56 px before subsequent H1/H2 sections, 40 px before H3-H6,
  and 12-18 px after headings according to level. Insets and table/card padding
  remain separate from these external gaps.
- The actual `plan.md` is covered by a native-font planner regression asserting
  the labelled split and byte-identical serialization. The synthetic
  `51-editorial-decision-panel.md` fixture was visually inspected at 1600x1200,
  1280x1000, and 760x900. Wide/regular views form the split; the narrow view
  preserves the vertical source-order fallback. All three native captures pass
  the unchanged-source oracle. Evidence: `editorial-decision-{wide,regular,narrow}.*`.
- Production binary SHA-256
  `6172aaa44acce5c04dccec51a6b885b1bc62200cc00e03c4334ef64b81ebc5a9`
  passes the 10 MiB continuous-input regression at **109.2 FPS** with **4.15 ms
  draw p99**. The complete check set passes 440 Rust tests (two optional
  installed-font tests ignored) and 55 Python oracle tests. Evidence:
  `editorial-decision-long-scroll.{json,log}`.

### Document-wide labelled-list compositions

- The automatic selector now recognizes repeated authored `**Label:**` and
  `` `TechnicalLabel`: `` prefixes throughout a document. Three labelled items
  nominate one measured three-card row; longer groups nominate two or three
  columns according to their native wraps. One unlabelled summary item is
  tolerated when at least three quarters of the group retains the repeated
  structure.
- Labelled groups receive quiet cards with one green top rule. Bold labels use
  the document's restrained Fraunces card-heading treatment; inline-code labels
  remain monospaced. Ordinary short lists retain the existing neutral measured
  grid, while uneven prose lists and complex/nested content remain vertical.
- The earlier introduction/list split now requires an actual short authored
  colon label before the list. This prevents generic section paragraphs from
  being repeatedly pushed beside unrelated bullet lists.
- The real `plan.md` planner regression requires its opening decision panel and
  at least three additional labelled list groups to compose, with byte-identical
  serialization. Fixture `52-labelled-list-compositions.md` covers three-card,
  six-item specification, uneven vertical-list, and ordered-step decisions.
- Inspected native captures at 1600×1200, 1280×1000, and 520×1000 demonstrate
  three columns, two columns, and the narrow vertical fallback respectively.
  Every capture passes the unchanged-source oracle. Evidence:
  `layout-previews/labelled-compositions-{wide,regular,narrow}.{png,source.json,planning.json,log}`.
- Production binary SHA-256
  `4ccc3530d79660c9e3433cebca35a44e2b06f426d2b7e3239cbb026250546bb8`
  passes the isolated 10 MiB continuous-input gate at **109.4 FPS** with
  **3.99 ms draw p99**. The complete check set passes 443 Rust tests (two
  optional installed-font tests ignored) and 55 Python oracle tests; formatting,
  all-target checks, warning-denied Clippy, and diff whitespace are clean.
  Evidence: `layout-previews/labelled-compositions-long-scroll.{json,log}`.
