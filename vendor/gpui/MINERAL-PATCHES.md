# Local GPUI patches

Upstream: Zed `8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc`, package GPUI
0.2.2, Apache-2.0. The license is an exact copy of the upstream root license
(the package originally linked to it). Original examples and assets are
retained; the production Rust changes are documented below.

Only `Cargo.toml` is normalized: upstream workspace package fields, lints and
dependencies are expanded; upstream sibling path dependencies keep their
original Git source identity. The root `Cargo.lock` pins their revisions.
The root workspace patches GPUI to this directory without upgrading it.

Before/after complete workspace Cargo metadata has 968 resolved nodes and
identical dependency edges and features when only GPUI's source location is
normalized. The normalized graph SHA-256 is
`3249e38f00c9f63295c6baaebe90a9e63c4689c3c228c05a774add0f447a1b0a`.

This package now contains Mineral's retained-accessibility implementation from
`performance/RETAINED-ACCESSIBILITY-PLAN.md`. It remains excluded from ordinary
workspace lint/test ownership; the focused production harness under
`performance/a11y-publication-tests` compiles these exact sources and exercises
them against the pinned AccessKit consumer.

## Retained accessibility subtree (2026-09-11)

`RetainedA11ySubtree` owns one validated immutable hierarchy plus sparse
property overrides. A frame that keeps the same attachment publishes no
unchanged retained nodes; replacement, reattachment, and activation publish a
complete baseline. Complete debug snapshots still merge the delta with retained
state. Release builds keep the debug accessibility cache disabled unless
`GPUI_A11Y_DEBUG` is set, avoiding a second full-tree baseline in production.

The focused retained-subtree tests cover invalid hierarchy rejection, unchanged
zero-scan frames, sparse overrides, detach/reattach, replacement and forced
activation. The document-view native harness covers real offscreen semantics,
action dispatch and source preservation.

`RetainedA11ySubtree::revise_properties` lets the owner replace the complete
property baseline while keeping the retained publication identity, but only
when the node IDs and every child list are unchanged. The owner still submits
each changed node as a sparse update. Later reattachment or full publication
therefore uses the latest baseline rather than the properties from the first
revision. Any topology change is rejected and must create a new retained
subtree. `retained_property_revision_keeps_identity_and_latest_full_baseline`
covers the delta, unchanged follow-up, and reattachment behavior.

## Retained accessibility debug snapshot (2026-09-10)

`window/a11y/snapshot.rs` retains equal nodes in the complete debug TreeUpdate
instead of cloning every property buffer on every frame. Equal-length snapshots
compare both IDs and values; size changes replace the complete snapshot. Tree
metadata, tree identity and focus are always refreshed. This does not change
platform publication, drop offscreen semantics, disable inspection or alter the
public debug API. The production helper is tested in
`performance/a11y-publication-tests` for unchanged storage during scroll and exact
snapshot/consumer behavior through editing, reorder, insertion and removal.

## Synthetic subtree action dispatch (2026-09-10)

`Window::on_a11y_action_request` registers a frame-local synthetic subtree
handler. Exact node/action listeners retain precedence; subtree handlers decline
unknown targets and actions, then built-in handling remains available. Subtree
registrations expire in `begin_frame`, alongside node-specific registrations.
The document editor shares its immutable compiled action map with one handler
instead of allocating callbacks for every semantic node on each paint. Source
and geometry changes rebuild that map through the existing semantic cache.
Native fixture 27 exercises this GPUI dispatch path through the actual AT-SPI
consumer, including offscreen reveal, task click, undo and reactivation. This
native harness supplies integration coverage for the API; upstream GPUI's own
unit suite remains outside workspace ownership.

## Disabled accessibility metadata (2026-09-10)

`StatefulInteractiveElement::aria_disabled` carries the standard disabled flag
through `AriaProperties` to the AccessKit node. It is metadata only; callers
must still suppress input. Mineral's `ButtonAccessibilityExt::accessible_disabled`
sets both toolkit interaction state and this flag on the existing button node.
Native title-bar zoom-limit tests verify the end-to-end AT-SPI state and ignored
pointer activation. The AT-SPI adapter also needs the companion disabled-state
conversion fix documented in its own patch notes. Remove this property override
when the pinned GPUI provides the equivalent API/publication behavior.

## Linux HTML clipboard alternative

`ClipboardString` can retain a separate HTML alternative on Linux, exposed by
`ClipboardItem::with_html` / `html`. Text and private metadata retain their
existing meanings. The field is Linux-only so pinned macOS/Windows providers
keep their existing constructor layout. This is an outgoing native MIME bridge;
it does not add foreign HTML clipboard import or change primary selection.
Remove when upstream offers the equivalent explicit format transport.
