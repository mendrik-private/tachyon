# Clustered margin notes

2026-09-10. A07 / STR-0003 layout-first continuation. The bounded cluster
implementation below is verified; the full audit remains active.

Adjacent top-level blockquotes explicitly labeled `Margin note:` can share the
same preceding paragraph anchor. Each note keeps its canonical identity, full
text and quiet note typography. A heading, ordinary quote, critical alert or
other source block ends that relationship. No distant anchors are invented.

Measure the complete cluster in one side rail, in source order, with normal
quote padding/gaps and the existing prose measure. Reject the entire rail if it
does not fit the width/height/readability or bounded-search budget; never move
only the first notes and strand the rest. Existing Note/Tip cluster behavior
remains unchanged. Narrow/short/zoomed views put all notes inline after their
paragraph. Accessible descriptions contain every anchored note once.

Verify canonical relationships, all-note geometry/nonoverlap, fallback barriers,
focused later-note typography, exact save/Undo and native mixed-document views.
Arbitrary distant/HTML/nested/RTL anchors, paged placement and the complete
interaction/performance matrix remain part of the full audit.

## Implementation and regression evidence

Fixture 110 initially produced only one anchored note; the following two used
ordinary quotation typography and the whole sequence stacked. Projection now
propagates the paragraph anchor only through consecutive source-valid explicit
margin notes. Ordinary quotations and other source blocks still break the chain.

The complete rail is measured as one source-order part. Row legality permits
an Aside to consume up to the existing 40-unit search window, while other row
kinds keep their three-unit limit. The selector delegates that rule to the same
legality predicate instead of duplicating a three-unit cap. A canonical next-root
check rejects a cluster cut by the window. All existing width, overflow, height
and balance limits remain; general Note/Tip clusters still have their two-note
limit. A rail never moves a partial cluster beside the paragraph.

The first measured test remained red after recognition changed: a valid 7/5
candidate with 192/264px measured heights was excluded by the old group limit.
Both legality and selection now accept that complete candidate. No fit rule was
weakened. `margin_note_cluster_uses_one_complete_rail` covers loaded-font
100/150/200% geometry, three nonoverlapping quiet note rows, narrow/short fallback
and return to a rail when space is restored. Search-window tests cover 3, 39, 40
and 41 notes, including truncated-window rejection.

A second red test showed that editing the first note's label released the
following notes' typography. Focused-role retention now reconnects only its
contiguous, still-source-valid suffix. First/middle/last edits preserve all three
identities until blur; blur re-evaluates the actual labels. A barrier quotation
still prevents an unrelated later note from acquiring an anchor. Undo is exact.
Semantic tests verify one anchor description with the three canonical note
paragraphs once each in source order, at narrow and wide widths.

The retained-row typing/full-geometry matrix now includes each cluster note at
360/1280px and 100/150/200% text scale. Growth and shrink preserve the canonical
layout and keep its existing per-transaction measurement bound. Its expected
full layout applies the same focused quote-role contract as production. This is
bounded-work regression coverage, not an end-to-end performance qualification.

## Native verification

Runtime SHA-256:
`0c0b521fe06af14b6443e44186bdb7cb3ce57fb46b46ac94391cc026bba87d4e`.
Fixture SHA-256:
`4f4512aefa53edb3266a18c92a44f2611ad7f1a6e60444075fa3add374a964cc`.
All captures used private copies of fixture 110 in the Wayland harness.

Under `performance/layout-previews/`:

- `note-cluster-before`: original `156af6f6…` binary; only the first note is quiet
  sans text, all three stacked. Source unchanged.
- `note-cluster-clean`: final 1600×1700 light capture, no selection. The complete
  rail occupies previously unused space and brings the next section upward.
- `note-cluster-wide-final`: same size, source-order copy probe (therefore the
  screenshot has a selection). Seven markers appear once and in order. Native
  AT-SPI bounds place the main paragraph at x259/y314, width548/height192;
  note text rows share x831, y330/426/522, each height40. Their 24px minimum
  column gutter and nonoverlapping vertical order remain intact.
- `note-cluster-narrow`: 620×1700 light, all three notes inline after the anchor.
- `note-cluster-dark200`: 1100×1900 dark at native 200% zoom, all three notes
  inline and visible; scrolling remains available for the following section.
- `note-cluster-first-edit` and `note-cluster-later-final`: native insertion at
  the first and second note labels, one-second focused idle, exact complete
  expected saved source, autosave and byte-exact Undo. Focused captures retain
  every note in the same rail with the same typography.

The clean, narrow, dark-200 and both focused-note idle saved images were visually
reviewed. Whole-file unchanged checks pass for the view/copy captures. A first
later-note expectation omitted the edited paragraph serializer's escaped comma;
the corrected whole-file expectation passes without a production change.
The intermediate `note-cluster-wide` capture predates the selector fix and is
diagnostic only. The first full-suite attempt used an incorrectly extended test
target filter (margin notes are not deep list items); the corrected matrix runs
the actual margin-note targets and passes independently.

These checks do not qualify arbitrary distant, nested, HTML or RTL margin-note
anchors, multiple paragraphs per note in every interaction, native continuous
resize/IME/drag paths, long-cluster latency, pagination or printing. E08 remains
partial, and the complete source/edit/media/page/export/performance audit stays
active.

## Final checks

`scripts/check.sh` exits 0: pinned-source/locked metadata, formatting, all-target
workspace check, strict Clippy, 714 passing Rust tests and doctests. Two existing
document-view tests remain ignored. `git diff --check` passes. The focused
margin-note run passes all 11 tests; the extended retained-row typing matrix
passes both independently and in the full suite.

Logs: `/tmp/tachyon-note-cluster-{red,decision,focus-red,green,typing,check}.log`
and the corresponding native capture logs. Crusty context `ctx_a29135bab6f2`,
validation `task_f4fd0f4c16ef1ee8`: 37 existing advisory findings, no new or
worsened findings. Existing advisory debt is not claimed resolved.
