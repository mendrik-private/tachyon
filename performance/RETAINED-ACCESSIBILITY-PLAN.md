# Retained accessibility publication

## Problem and evidence

The layout-first audit requires complete canonical reading order and stable
actions without whole-document work on scroll. Current native active-AT-SPI
100 KiB scrolling is below 60 FPS; 10 MiB runs can disconnect before reporting.
See `CURRENT-LAYOUT-PERFORMANCE.md` for runtime identities and limitations.

The application already caches semantic geometry and compiled native nodes.
Nevertheless, `editor/accessibility.rs::SemanticTree::publish` clones every
node and action target for each frame. Pinned GPUI `window/a11y.rs` accepts
owned nodes through `A11ySubtreeBuilder::push_child`, reconstructs the complete
tree, validates it, and submits it through `Window::draw`. Its
`window/a11y/debug.rs::A11yDebug::capture` also clones the entire update in
release builds. The adapter then processes that full update. The symbolized
profile shows consumer updates, text traversal, allocation and copies among
the dominant sampled functions; it does not prove a single sole bottleneck.

This is an ownership/protocol boundary problem, not a reason to expose only
visible semantic nodes or to disable accessibility. A backend-only node diff
would leave the frontend's full construction and debug copies in place.

## Intended boundaries

```text
document-view: canonical semantic revision + published geometry + actions
    ↓ immutable retained subtree, new only when its content/geometry changes
GPUI: subtree lifetime, identity validation, focus, frame attachment, deltas
    ↓ standard AccessKit TreeUpdate
AT-SPI adapter: platform semantics and native notifications
```

`document-core` remains unaware of accessibility and display caches. No second
document model, persisted layout preference or independent source order is
introduced. `markdown-app` continues supplying environment/window geometry.

### Application ownership

- Retain one immutable semantic subtree for an authoritative published
  geometry revision. Content, zoom, reflow, disclosure, task, math/resource
  changes invalidate the relevant tree; ordinary scroll changes its ancestor
  transform only.
- Preserve all canonical IDs, labels, hierarchy, roles and actions, including
  offscreen content. Never substitute a sparse viewport tree.
- Retain action registrations by subtree lifetime. Callbacks must resolve the
  current editor/node geometry, not keep stale source bounds after a reflow.
  Use the existing weak editor ownership and release registrations on removal.
- Measure initial construction separately. Moving linear work to a single
  blocking activation frame is not sufficient; keep large-document opening
  responsive and qualify complete readiness before timing steady scrolling.

### Framework ownership and API direction

Evaluate a small retained-subtree API in pinned GPUI, rather than encoding
private metadata into AccessKit roles, values or IDs. A candidate API has an
immutable validated subtree value and an explicit builder attachment method.
This is a design direction, **not an already available GPUI API**.

The framework owns:

- Validation at subtree construction/replacement: unique IDs, valid children,
  acyclic ownership, one canonical parent and valid roots. Fail explicitly for
  invalid external references; do not silently strip retained descendants.
- Per-frame attachment and liveness by subtree identity, without rewalking
  every descendant. Duplicate IDs across live retained and ordinary nodes,
  stale focus IDs and released subtrees must remain detectable.
- Full initial publication, then changed ordinary nodes/ancestor transforms
  and new/replaced subtrees. Reconnect/reactivation requires a valid full
  baseline, not deltas against an unknown adapter state.
- Correct subtree detach/removal and reattachment, including focus/actions.
  Do not treat an omitted unchanged node as a deleted node.
- Debug inspection backed by retained state and materialized on request, not
  a full owned-tree clone on every release frame.

The AT-SPI adapter keeps consuming the normal AccessKit protocol. Do not add
a Mineral-specific parallel adapter or duplicate native application/window.

## Incremental implementation and proof

1. Inspect all pinned GPUI tree-builder, debug snapshot, activation, focus and
   action consumers. Confirm whether a retained API exists before adding one.
   Current inspected synthetic builder exposes only owned-node insertion.
2. If a local GPUI patch is necessary, preserve the exact Zed source revision,
   licenses, dependency features and resolved type identity. Normalize only
   the package's inherited manifest paths; do not upgrade GPUI or patch the
   shared Cargo registry/checkouts. Check metadata and source pins before
   changing renderer call sites.
3. Add framework tests comparing retained/delta publication to a complete-tree
   reference through AccessKit's actual consumer. Cover transform-only frames,
   edits, insert/remove/reorder, reparenting, nested subtrees, repeated attach,
   activation/deactivation, invalid IDs/children, focus and resource release.
4. Integrate the immutable application semantic tree, retaining source-owned
   order and current action resolution. Delete the per-frame complete-node
   publication path once the replacement is validated; no indefinite dual
   implementation or inaccessible fallback.
5. Require deterministic counters proving zero unchanged-descendant clones,
   scans and registrations on a scroll frame. Also prove that changed content
   and geometry are actually published and old resources are released.
6. Run native AT-SPI traversal, action, disclosure/math, selection/edit/Undo,
   source fidelity, resize and zoom checks with complete canonical coverage.
7. Profile and run matched optimized 100 KiB/1 MiB/10 MiB workloads, then the
   specified repeated 60-second qualification. Report median/tails, stalls,
   actual scale, source/runtime identity and memory. Keep the physical-display
   release protocol distinct from isolated Weston diagnostics.

The acceptance criterion remains the full layout/interaction/performance
contract. A retained API, passing unit test or smaller tree by itself is not
completion. This plan does not authorize new product UI or a source-format
change and does not mark any audit item complete.
