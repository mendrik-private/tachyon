# Mineral's GPUI Linux patch

Source: `crates/gpui_linux` from `https://github.com/zed-industries/zed`,
revision `8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc` (Apache-2.0;
see `LICENSE-APACHE`). Source files were copied unchanged before the patch below.
The standalone manifest expands that revision's workspace dependencies and lints.
Zed dependencies retain the root workspace's Git source identity; `Cargo.lock`
pins them to the same revision. Use locked builds. Do not independently add `rev`
queries here, which would create distinct copies of the GPUI types.

## Patch

In the Wayland keyboard handler, XKB composition cancelled by Escape clears the
backend's pending accent and resets the compose state, without inserting text.
The ordinary Escape key event still reaches the focused editor so its existing
cancellation action can restore the original content and selection. Other
cancelled sequences retain upstream's printable-accent fallback and dead-key
restart. No unsafe code is added or changed.

## Regression

`performance/capture-layout.py --edit-compose-cancel-check` exercises real
Wayland dead-key preedit, Escape, exact selected-text restoration, unchanged
saved source, restarted preedit, commit and one-step undo on private fixtures.
See the adaptive implementation ledger for the full command and evidence.
The unpatched release fails the exact-source and restored-selection assertions.

Remove this override when the pinned upstream handles Escape cancellation
without committing pending text and passes the same native regression. This
patch does not claim to fix all input-method protocols or CJK candidate windows.

## Incremental Wayland accessibility publication

The pinned GPUI `A11y::end_frame` produces a complete tree on each active frame.
Wayland now compares that complete publication with the last tree delivered to
its AccessKit adapter, forwarding only changed nodes. Omitted nodes remain in
the consumer; changed parent child lists remove subtrees. Missing IDs are also
forgotten by the publisher so later ID reuse sends a new node. Focus and tree
metadata are forwarded unchanged. This does not virtualize accessibility or
remove offscreen content, and does not alter action routing or scrolling.

Each activation forces a complete baseline, including reactivation following
deactivation. The reset and its consumption occur inside AccessKit Unix 0.21.1's
activation/update callbacks, which share the adapter-state mutex. The publisher
never advances while the adapter is inactive. Recheck this locking and GPUI's
complete-frame contract when updating dependencies; do not feed partial frames
to this cache.

`performance/a11y-publication-tests` includes the production publisher directly
and tests it against the pinned AccessKit consumer: all offscreen descendants,
scroll transforms, Unicode edits, focus, removal, reordering, reparenting, ID
reuse, and complete reactivation. Native timing and semantic evidence are
recorded in `performance/REFERENCE-SCROLL-FIX.md`.

On forced full publication the cache moves the complete tree into AccessKit and
clears its comparison baseline, avoiding a second full-tree clone during
activation. The frontend retained subtree and complete debug snapshot remain
available. Remove this publication override when upstream GPUI provides
equivalent incremental updates and passes these regressions.

## Fractional scale precedence on older surfaces

Output Enter/Leave events still update output membership and subpixel layout,
but must not reset the surface scale when the fractional-scale manager exists.
The old wl_surface-version fallback could overwrite a fractional preference
with an integer output scale, depending on event order. PreferredBufferScale
already observed this protocol precedence; Enter/Leave now do too. Compositors
without fractional scaling retain their legacy output-scale path.

The native title-bar harness compares the actual accessible 28px button bounds
against the requested display scale before accepting a screenshot. See
`performance/TITLE-BAR-STYLE.md` for the repeated native matrix. Remove this
override when the pinned upstream preserves the same preference across output
membership events and passes the matrix.

## Wayland HTML clipboard publication

A normal clipboard item with an explicit GPUI HTML alternative advertises
`text/html` alongside the existing plain-text aliases. Send dispatch selects
that requested representation; unknown MIME requests do not receive plain text.
Private same-process metadata and primary selection remain unchanged. A unit
regression exercises format dispatch; `performance/html_clipboard_check.py`
inspects actual `wl-paste --type` payloads on an isolated Wayland seat. This does
not implement incoming foreign HTML or X11 clipboard formats. Remove when the
pinned upstream supports equivalent outgoing alternatives.
