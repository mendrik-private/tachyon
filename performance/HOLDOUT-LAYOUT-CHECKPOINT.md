# Real-document layout review and capture provenance

2026-09-09. Layout-first A07 continuation. This is current-build held-out
evidence, not full board sign-off or completion of the document grammar.

## Scope and result

The actual repository `plan.md` and `README.md` were staged byte-for-byte in
private Wayland sessions. Their content was not shortened or rewritten to
qualify for a layout. The editable app receives only the staged path; relative
resources are deliberately not imported by this harness route.

Runtime SHA-256:
`fb1d400ecb29252f01557a1653ff17270b715ccc31cfe1263889d2f5da76d1a5`.
No Rust/runtime code changes were made in this review. The previous full
workspace check remains the runtime test evidence; this turn adds independent
native held-out observations and Python evidence-harness tests.

The inspected wide plan uses an introduction/decision-list band and a complete
typography explanation beside its table. Its long, uneven window-layout list
stays vertical; that is a readable fallback, not a reason to stretch all prose.
The component table below uses the available width. Narrow plan views stack
the same content, while a short window rejects tall compositions. The README
retains narrative typography in its opening, reference typography in its
technical section, and code labels above their examples. At 200%, its opening
and headings remain readable without narrowing type to preserve columns.

The final plan traces report no planner failure or invalid row windows:
1314px canvas/31 chosen rows at wide width, 496px/33 rows at narrow width,
and 1006px/33 rows in the short-window check. These counts describe the reviewed
revisions, not a new universal layout acceptance threshold.

## New typography finding

`current-readme-dark-200.png` shows **`undo` at the end of one body line and
`/redo,` at the beginning of the next**. The original README contains the
continuous `undo/redo` token; this is not an authored hard break or lost source.
It is a visible punctuation-sensitive wrapping issue, left open under F09.

Source inspection locates ordinary LTR wrapping in
`document-view/src/editor/measurement.rs::FontMeasurement::wrap`, which consumes
the pinned GPUI `shape_text` boundaries. GPUI's `LineWrapper::is_word_char`
includes several joiners and trailing punctuation but not `/`. This is a
plausible root-cause boundary, not a complete diagnosis of every punctuation
case. The next layout work should reproduce the case at the measured text
boundary and correct line-break behavior without modifying source, splitting
graphemes, breaking RTL, or introducing scroll-time shaping.

## Source identity and external edits

The plan changed externally between captures. The initial wide capture has
SHA-256 `c2b4902d2cadff4a628c28daabf7cfe8438e8cd49685db0b735a9c4f13fc8fc4`;
the later middle capture has
`eb606e7f3ba6116844845694d78353bdb9a1a653c4cb889b6337187b9fb51c99`.
Every private-copy source check passes, but those images are **not a controlled
same-source width comparison**. Consult each `.source.json` rather than infer
identity from the shared filename. No original document was an editable target.

The capture harness now records a `source_origin` object for checked holdouts:
the pre-staging resolved source path, exact staged hash, current original hash,
`unchanged`/`changed`/`unavailable` state at verification, and explicit
`selected_markdown_only` resource scope. Origin drift is separate from the
private app copy's `source_unchanged`/`passes` result. An external edit/deletion
does not falsely become evidence of app corruption, nor is it silently treated
as a controlled same-source comparison. No deleted or changed original is
recreated by the checker. The new fields do not retroactively appear in older
captures.

README stayed at SHA-256
`abca9115a24f8b4293074b510011acfc5d11a036f910ba68d77e6f0054527541`.
The two new provenance-bearing native records confirm both the staged and
original file hashes at the end of their runs.

## Native artifacts

All files below are under `layout-previews/`; native pixels were inspected.

| Prefix | Coverage |
| --- | --- |
| `current-plan-wide` | 1600×1400 light; opening, decision/list split and typography table pair. |
| `current-plan-middle` | 1600×1400 light, 18 wheel steps; authored table treatment, long hanging list and wide component table. Different plan revision from the initial wide capture. |
| `current-plan-narrow` | 520×1400 light; wrapped title, opening, source-order decisions and next section. |
| `current-plan-short` | 1280×480 light; short-window single reading flow and separate navigator. |
| `current-readme-wide` | 1600×1400 light; narrative opening, explanation/code pair, labels and command strip. |
| `current-readme-narrow` | 520×1400 light; readable stack, preserved code overflow and command strip. |
| `current-readme-dark-200` | 1280×1400 dark at 200%; enlarged opening and the punctuation-wrap counterexample. |
| `current-readme-command-edit` | Native Home/type in the command strip, full edited-file equality (`scripts/check.sh` → `xscripts/check.sh`), autosave, one-second idle and byte-exact undo. Original and staged provenance both unchanged after undo. |
| `current-readme-qualified-copy` | Six unique section markers each occur once in source order; staged/original provenance unchanged. Not a whole-clipboard equality claim. |

The initial `current-readme-provenance-copy` probe supplied the nonexistent
heading “Reproducible dependency pins”; the real heading is “Reproducible
dependency policy”. Its failure is a probe-oracle error, not an editor defect.
The corrected marker check passes without changing the README.

## Verification and remaining scope

`python3 -m unittest discover -s performance -p test_capture_layout.py` passes
16 tests, including unchanged/changed/deleted origin cases and the existing
private-copy/source-isolation checks. Log: `/tmp/mineral-holdout-tests.log`.
Native code editing compares the entire autosaved file, not only the inserted
marker. The capture origin reader is read-only and reports unavailable originals
without masking the independent private-copy result.

The UX skill guided the width/height/text-scale review and separation of readable
prose from wide components. Rust engineering guidance informed the requirement
to use actual runtime behavior and retain source/geometry evidence rather than
infer correctness from a green suite.

F09 punctuation wrapping remains open. These views do not qualify all seven
boards, relative-media loading, arbitrary scripts, every interaction/state,
anchored margin notes, playable media, page masters/exports, or controlled
release performance. The full goal and A07 remain active.
