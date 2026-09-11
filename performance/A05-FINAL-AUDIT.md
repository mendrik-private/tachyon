# A05 final audit — stale file and layout completions

Work item: `work_2abc3d917b108c1a`

## Previous failure

Document revisions and view generations restart for each document. Two different
files could therefore have equal revision and generation values, while file and
layout completion guards had no durable session identity to distinguish them.
Save completion also updated the path-keyed session registry without proving
that the registry still owned the document that initiated the save. A file read
captured metadata from its open handle, but did not verify that the pathname
still referred to that handle after the bytes were read.

## Final behavior

`SharedDocumentSession` now assigns one process-local `DocumentSessionId` and
retains it across clones and in-session document replacement. Open, chooser,
reload, recovery, watcher, overwrite, save, outline and autosave/recovery timer
completions capture and validate their relevant session, epoch, revision,
request, path or identity fields on the UI thread before changing window state.
Prepared reload candidates retain their originating session, epoch and path.

Layout reflow keys include the session ID in addition to document and geometry
generations, viewport dimensions, zoom, resources and focus. A held worker from
another session is discarded even when the two sessions have identical initial
revision and generation values. Layout cache reset and worker ownership remain
unchanged: switching documents does not abandon the worker, and its eventual
completion cannot publish into the replacement session.

The session registry now claims, completes and releases saves only for the exact
`SharedDocumentSession` that started them. Successful save ownership is resolved
in the registry before the optional window update, so closing the initiating
window cannot strand or redirect the registry state. A newer edit in the same
session remains dirty after the older revision is saved.

Load and save workers preserve typed persistence errors through their worker
boundaries. Source reads derive bytes and fingerprints from one open handle,
compare before/after handle metadata, and verify after the read that the source
path still identifies that same file. Unix device/inode identity plus SHA-256
content fingerprints distinguish replacements that reuse length or timestamps.

## Regression evidence

- Separate sessions with equal revision/generation receive distinct IDs; clones
  retain the same ID.
- A deterministic oneshot barrier holds a real reflow through a session switch;
  releasing it leaves replacement content and geometry intact.
- Completion-ticket tests reject changed request, session, epoch, revision and
  source path values for identical-initial-revision documents.
- A stale save completion cannot clear or update a replacement path session.
- A deterministic read hook replaces a pathname after bytes are captured; the
  read returns typed `PersistenceError::ExternalChange` and never pairs the old
  bytes with the replacement identity.
- Existing save-stage injection still proves that an external edit immediately
  before replacement wins and returns the typed conflict.

`scripts/check.sh` passes 832 tests with two existing ignored tests, including
locked workspace checks, formatting, strict Clippy, the patched accessibility
adapter suites and doctests. The complete log is
`/tmp/mineral-a05-check.log`; the two Unix-socket tests were run outside the
filesystem sandbox because the sandbox denies socket binding. `git diff
--check` is clean.

A06 still owns broader native workflow presentation and close/recovery policy.
A07 and later layout work remain open; this result establishes the completion
identity and publication boundary they depend on.
