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
