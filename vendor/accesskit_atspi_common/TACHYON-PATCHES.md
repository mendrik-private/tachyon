# AccessKit AT-SPI local patches

Base: crates.io `accesskit_atspi_common` **0.18.1**, upstream commit
`f40dfc01a0c0e76de535969f82fb35e19513737d`, `platforms/atspi-common`.
The published sources, normalized Cargo manifest and README are retained.
Registry bookkeeping and the upstream lockfile are omitted. Original source
copyright notices and MIT, Apache-2.0 and Chromium licenses are retained.

Production changes are in `src/node.rs` and `src/adapter.rs`.
`NodeWrapper::state` translates the existing
AccessKit `is_expanded()` property into AT-SPI `Expandable` and `Expanded`:

- absent: neither state;
- `Some(false)`: expandable, not expanded;
- `Some(true)`: expandable and expanded.

The existing state-set diff in `notify_state_changes` emits the corresponding
native notifications. This patch does not relabel controls, mark them checked,
alter actions, replace nodes, or change the application's content model.

The regression failed before the mapping and covers all three property states:

```sh
cargo test --locked -p accesskit_atspi_common expansion_maps -- --nocapture
python3 performance/capture-layout.py --fixture 28-accessible-html.md --width 1280 --height 1000 --startup-wait 8 --atspi-html-check --output /tmp/tachyon-expanded.png
```

The native check requires expanded/collapsed state, state-change notifications,
stable control identity, source-order visible content, authored/nested open
state, and unchanged original HTML after open/close. It runs only on the
capture harness's private Wayland and accessibility buses.

Remove this patch when a compatible published adapter includes this mapping,
then rerun both regression layers. No registry or upstream checkout is edited.

## MathML element tags

`NodeWrapper::attributes` also maps the existing AccessKit `html_tag` field to
the AT-SPI `tag` object attribute. No new public API, interface, unsafe code or
document state is introduced. The application publishes a bounded MathML child
tree from its rendered formula AST, retaining the original LaTeX label.

This follows the native consumer contract in
[Orca's MathML reconstruction](https://github.com/GNOME/orca/blob/main/src/orca/ax_utilities_math.py):
it obtains each element name from `tag` and token text from its accessible name,
then walks children in source order. AccessKit's `inner_html` alone is not
currently forwarded by this Unix adapter. The app also supplies that field for
other compatible consumers, without claiming those platforms were tested.

AccessKit `Role::Label` tokens must supply `Node::value`, not `Node::label`.
The adapter's existing `name()` behavior is correct and remains unchanged.
The first native fixture check caught empty numerator/denominator names despite
correct tags; the application's token constructor was corrected and has a
regression in `document-view/src/editor/accessibility.rs`.

`cargo test --locked -p accesskit_atspi_common mathml_tag -- --nocapture` failed
before the mapping and passes after it. Native verification uses only the
harness-owned accessibility bus:

```sh
python3 performance/capture-layout.py --fixture 40-accessible-math.md --width 768 --height 1200 --startup-wait 8 --atspi-math-check --output /tmp/tachyon-math.png
```

The probe reconstructs fractions, indexed radicals/scripts, matrices and limit
operators from actual AT-SPI nodes, checks token order and exact source labels,
and requires invalid formulas to retain source-only fallback. It does not claim
an audible Orca/MathCAT or braille-device test. Remove this mapping once the
pinned upstream adapter supplies equivalent tags, then rerun the native check.

## Native text-notification regressions

The adapter tests require no text-change notification on a geometry-only
translation, exact Unicode insertion/removal notifications during a concurrent
parent translation, and inserted/removed text-run notifications. They exercise
the real adapter/change handler and callback events, not a parallel string-diff
implementation. These are correctness tests, not a claim of bounded text scans.
`scripts/check.sh` runs this dependency's tests explicitly because it is
excluded from the application workspace members.

```sh
cargo test --locked -p accesskit_atspi_common
```

A geometry-only text-check shortcut was evaluated and removed: three short
before/after pairs and a 60-second pair did not establish a reliable end-to-end
benefit. The rejected candidate passed its own scan-count regression, but that
was insufficient evidence to retain it. No production adapter optimization is
left from that experiment. See `performance/CURRENT-LAYOUT-PERFORMANCE.md` and
`performance/RETAINED-ACCESSIBILITY-PLAN.md` for measurements and the intended
framework/application boundary. Full native/performance coverage remains open.

The retained-tree implementation instead separates the immutable
`Role::Document` text owner from its scrolling geometry. GPUI publishes the
document beneath a transparent `Role::GenericContainer`; scrolling changes only
that container. The adapter forwards one bounds invalidation to each first
exposed descendant when such a transparent ancestor's direct transform changes.
It therefore preserves current descendant extents and native coordinate cache
invalidation without asking `document_range()` to concatenate the whole tree.

`transparent_geometry_scroll_is_bounded_and_invalidates_document_extents`
uses 3,000 text runs and verifies zero document-text comparisons plus exactly
one exposed-document bounds event. The existing Unicode edit, text-run
insert/remove, and geometry notification tests remain the correctness boundary.
This is a structural ownership change, not the rejected property heuristic.

## Linear child-membership notifications (2026-09-10)

`NodeWrapper::notify_children_changes` compares the fully filtered child lists
before performing membership work. Equal lists return immediately; changed lists
use sets for membership while emitting additions in new-list order with their
original indexes and removals in old-list order. This removes quadratic scans
without skipping visibility filtering or changing event semantics. Pure reorders
retain the adapter's existing no-add/remove behavior.

`filtered_child_notifications_preserve_order_indexes_and_visibility_changes`
exercises real `NodeWrapper::notify_changes` and adapter callback events for mixed
insert/remove/reorder, changed visibility with identical raw child IDs, and 3,000
unchanged children during a parent translation. The existing text-notification
regressions still cover concurrent Unicode edits and scrolling. This is separate
from the previously rejected text-check shortcut: no text checks are bypassed.

## Disabled button states (2026-09-10)

Disabled controls whose roles do not support ReadOnly must not fall through to
Enabled/Sensitive. The adapter now checks the disabled flag in that branch.
`disabled_buttons_are_not_enabled_or_sensitive` covers enabled and disabled
buttons using the real consumer tree and state conversion. Native title-bar
zoom-limit checks additionally verify the published state and ignored clicks.
Remove this override when upstream reports equivalent AT-SPI states.

## Retained large-document text (2026-09-11)

Tachyon publishes one marked, full-document `TextRun` before its semantic
object roots and an empty terminal `TextRun`. The terminal node bounds
AccessKit consumer versions that initialize both ends of their text iterator.
The adapter's equivalent text-capability check uses a forward descendant walk,
so an editor update finds Tachyon's first run at constant structural depth
instead of searching the complete semantic subtree.

When both old and new first runs carry the Tachyon marker and their values are
equal, child-only editor updates skip document-string reconstruction. A
wholesale text-length change above 64 KiB does not emit that payload as one
AT-SPI insertion/removal event; smaller and incremental Unicode edits retain
the adapter's exact prefix/suffix notifications. Ordinary filtered traversal
still hides TextRun nodes, leaving authored document-root positions unchanged.

`retained_text_makes_large_child_only_editor_updates_bounded` and
`wholesale_text_replacement_does_not_emit_an_unbounded_event` cover these
boundaries. Final private AT-SPI captures expose all expected 3,337 and 33,268
direct roots at 1 MiB and 10 MiB, hash representative nodes across the whole
document, preserve exact source, and pass the steady frame gate. See
`performance/RETAINED-ACCESSIBILITY-CHECKPOINT.md`.

For an incremental edit, old and new marked canonical runs are now diffed
directly. This avoids rebuilding the same full document string through
`document_range()` while preserving exact Unicode character offsets and
removed/inserted payloads. Unmarked trees keep the general consumer traversal.
`retained_text_edit_uses_direct_run_and_emits_exact_unicode_delta` changes one
multibyte character inside a 1 MiB run, requires zero document-range
comparisons, and checks the emitted Unicode events.

Accessibility may activate before Tachyon's first authoritative font layout
commits. The editor now publishes one empty `TextRun` during that interval, so
the adapter registers its AT-SPI `Text` interface with a valid degenerate range
in the initial object-server snapshot. The measured retained document replaces
that leaf on the next publication. This avoids both a late, unregistered Text
interface and `accesskit_consumer::Node::document_range()` on a text input with
no runs. `pending_text_input_with_empty_run_has_a_safe_native_text_range`
protects the adapter-facing shape; the A07 native matrix reads caret, character,
and selection state at all required fractional display scales.
