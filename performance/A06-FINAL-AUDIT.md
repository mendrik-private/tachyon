# A06 final audit — open, save, close, and recovery

Date: 2026-09-11

Work item: `work_218260a9ea2a7b3c`

## Completed workflow

The application exposes New, Open File, Open Folder, Save, Save As, Save Copy,
and Close through the shared window command context and application menu. A
first launch without a path opens a focused untitled editor. Open and save
chooser completions carry the originating session, document epoch, revision,
request, and path; cancellation leaves the document unchanged, while a stale
chooser completion cannot replace newer content.

Untitled documents receive stable recovery keys. Every committed edit schedules
a recovery write independently of the 750 ms autosave timer, including drafts
paused by a conflict. Active composition is deferred because preedit text is not
a committed edit. Recovery records are owner-only, use a versioned SHA-256 key,
carry the base disk identity, and sync their directory after publication and
removal. A malformed record cannot block the valid source from opening and is
retained with a visible warning. Restoring a file-backed record produces an
unsaved conflict that requires Reload, Save copy, or Overwrite; it is not
silently autosaved over the disk file.

Window close is an explicit state transition. Dirty or in-flight saves open a
modal Cancel/Discard/Save choice. Save waits for the current or newly requested
save and closes only after the matching revision commits. Any save or recovery
failure leaves the document open with its recovery actions. The watcher records
events that arrive during save/reload and reconciles disk identity after the
operation rather than assuming a re-armed watcher saw every change.

## Committed but uncertain writes

The remaining defect was the boundary after target replacement. A rename could
successfully install the new bytes and then fail while syncing the parent
directory. That path returned an ordinary I/O failure, so the UI could claim a
save failed even though the target already contained the requested revision.

Atomic replacement now captures the installed target identity immediately after
rename and returns a typed `DurabilityUncertain` warning if directory sync cannot
be confirmed. The save is treated as committed, the app adopts the reconciled
identity, and the status says that durability or recovery cleanup needs
attention. Its recovery record is retained. Failures before replacement still
return a failed save, preserve the original target exactly, and retain recovery.

Save As writes its recovery entry under the selected target and records that
target's base identity. A durability warning therefore remains discoverable on
reopen. Once target durability is confirmed, the target recovery and any former
untitled key are cleared by revision. Save Copy retains the current document's
recovery ownership and does not change the active path or source metadata.

Deterministic fault injection covers Create Temporary, Write, Sync Temporary,
Replace, Sync Directory, and recovery cleanup. The first four stages keep the
old file and report a recovered failure. A directory-sync failure reports a
committed save with the exact installed identity and retained journal. A cleanup
failure likewise keeps the committed identity and the still-inspectable record.

## Isolated native evidence

Final production release binary SHA-256:
`dc4b3b526743e31daa09be996efca9297042dcb124e9646e64f71d659e346dc6`.

Two private-Weston close runs make the fixture directory read-only, type an
unsaved edit, and wait past autosave. Both show the recoverable save error and
the modal close surface. Keyboard Cancel and Escape keep the app open and leave
the source untouched. The final Save run restores permissions, writes exactly
the edited source, and closes; the Discard run closes with the original source.
Reports are `layout-previews/a06-close-{save,discard}.unsaved-close.json`.

Two more private-Weston runs load a recovery record at 480 px, activate Restore
by keyboard, require all Reload/Save copy/Overwrite controls to remain keyboard
reachable, and resolve independently through Reload and Overwrite. Both retain
exact expected source bytes. Reports are
`layout-previews/a06-recovery-{reload,overwrite}.recovery-controls.json`.
The close, recovery, focus, and resolved-state screenshots were inspected.

Per the human's instruction, no physical desktop runner or physical input
injection was used. External system chooser presentation is accepted from the
existing platform integration and source-level cancellation guards rather than
opening a chooser on the working desktop.

Explicit saved table widths remain authoritative. Reading-edge alignment remains
limited to automatically sized tables. Files and Outline remain in the product;
the minimap remains excluded.

## Verification

Focused persistence tests pass, including the new stage-complete fault matrix.
The full `scripts/check.sh` passes: 129 document-core units, 54 source-fidelity
tests, 18 structural/tree-selection tests, 596 document-view tests with two
documented native-font ignores, 47 application tests, 12 vendored AT-SPI tests,
and eight retained-publication tests, plus formatting, locked checks, strict
workspace Clippy, and doctests. `cargo clippy -p markdown-app --all-targets --
-D warnings`, `cargo fmt --all -- --check`, and `git diff --check` also pass.
