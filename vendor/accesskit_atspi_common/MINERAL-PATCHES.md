# AccessKit AT-SPI disclosure state and semantic tags

Base: crates.io `accesskit_atspi_common` **0.18.1**, upstream commit
`f40dfc01a0c0e76de535969f82fb35e19513737d`, `platforms/atspi-common`.
The published sources, normalized Cargo manifest and README are retained.
Registry bookkeeping and the upstream lockfile are omitted. Original source
copyright notices and MIT, Apache-2.0 and Chromium licenses are retained.

Only `src/node.rs` is modified. `NodeWrapper::state` translates the existing
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
python3 performance/capture-layout.py --fixture 28-accessible-html.md --width 1280 --height 1000 --startup-wait 8 --atspi-html-check --output /tmp/mineral-expanded.png
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
python3 performance/capture-layout.py --fixture 40-accessible-math.md --width 768 --height 1200 --startup-wait 8 --atspi-math-check --output /tmp/mineral-math.png
```

The probe reconstructs fractions, indexed radicals/scripts, matrices and limit
operators from actual AT-SPI nodes, checks token order and exact source labels,
and requires invalid formulas to retain source-only fallback. It does not claim
an audible Orca/MathCAT or braille-device test. Remove this mapping once the
pinned upstream adapter supplies equivalent tags, then rerun the native check.
