# Successful-save source rebasing

A02 previously recorded successful-save rebasing as missing. The save worker now
prepares exact output bytes and a source spine mapped back onto the current
model's IDs. Installation happens only after a successful write. Ordinary Save
uses the originating shared document; Save As installs after successful adoption;
Save Copy does not modify the original document's source metadata.

Preparation imports the emitted bytes and compares the entire normalized model,
including nested items/rows/cells, text runs, metadata and container properties.
Only an exact match permits range remapping. The live tree and all its block
allocations remain unchanged. Parsed top-level units and nested source addresses
are remapped together, so later cell/list/quote edits can still preserve untouched
syntax. Parsing and comparison run in the existing background save worker.

The completion token retains its originating snapshot. Revision and
node-revision-map identity must still match; active composition also defers
installation. Selection-only movement is retained. Dirty serialization IDs and
the structural flag clear only when a matching source spine is installed.
Content/node revisions and shared-session generation do not change. Undo/Redo
entries keep their historical source spines and exact original bytes.

Markdown cannot represent every transient editing topology, including an extra
empty trailing paragraph. A failed exact-model match leaves metadata dirty and
keeps the previous spine; the requested file write still succeeds. This is a
conservative range-ownership guard, not normalization of live content or a claim
that the separate empty-paragraph persistence contract is complete.

Four source-fidelity regression tests cover:

- repeated saves/edits with mixed CRLF/LF, setext headings, lists and comments;
- node allocation/revision/selection preservation and complete Undo/Redo;
- stale edits, unrelated documents with equal revision numbers, selection-only
  movement, active composition and unrepresentable empty paragraphs;
- nested GFM cells, HTML table spelling, quotes, structural split/reopen, and
  subsequent edits after rebasing.

All 35 source-fidelity tests pass. Full workspace checks and native verification
are recorded below when complete. A02 remains active: source overlap,
replacement-boundary ownership and the complete structural matrix still require
qualification. This does not close A05's broader concurrent open/reload/save
lifecycle audit or establish large-document save latency/memory bounds.

## Verification

`scripts/check.sh` passes 799 tests, with two existing ignored tests, plus locked
checks, formatting, strict Clippy, adapter suites and doctests
(`/tmp/tachyon-rebase-final-check.log`). Diff whitespace checks pass.

The final release passes the native fixture 128 ten-case clipboard/first-edit
save/single-Undo check at 900×1000/100%, and fixture 44 four-case cross-owner
clipboard/first-edit save/single-Undo check at 1280×1100/200%. Both restored files
match their original source hashes. Reports (including exact binary SHA-256 and
MIME payloads) are `layout-previews/save-rebase-{html,cross}.{clipboard,source}.json`.
These exercise successful native autosave after conversion and historical source
restoration; deterministic core tests provide the direct source-spine/dirty-state
oracle. They do not qualify Save As chooser races or persistence failure handling.
