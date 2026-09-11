# Seamless HTML interactions checkpoint — 2026-09-10

Crusty work `work_7ccd1d0bea1a1724` remains incomplete. The accepted contract
requires ordinary selection and typing, complete canonical HTML roots in native
`text/html`, selected Markdown in `text/plain`, no mutation on copy, and atomic
conversion with the first edit.

The explicit HTML toolbar is removed, including its narrow overflow menu.
Preview measurement, pointer hit testing, caret placement, search reveal,
disclosure anchoring and test input coordinates no longer reserve its 32-pixel
height. Existing disclosure navigation tests now traverse two real disclosures
instead of relying on the removed toolbar as a neighboring focus target.

Validation:

- `scripts/check.sh`: 788 tests pass, two existing ignored; formatting, locked
  checks, strict Clippy, adapter suites and doctests pass.
- `python3 -m unittest discover -s performance -p test_find_check.py`: four pass.
  The native find oracle rejects obsolete HTML action buttons, with a negative
  test for each former action label.
- Native release SHA-256:
  `67ea388a8b40b20a1056d3b990afd3b4fa2a235b459493059ec4eb7c717691b2`.
- `layout-previews/html-seamless-find.*`: isolated Weston, fixture 41,
  600×1100, 100% text. Native search, pointer/keyboard match navigation,
  nested disclosure reveal, selected-text copy, first edit and exact source
  restoration after one Undo pass. Source hashes match. Screenshot inspected;
  the closed disclosure appears without an HTML toolbar.

Remaining implementation follows directly from current source:

- `RichDocumentEditor::copy` copies single-fragment HTML selections as plain
  editable text. Cross-preview copy derives a converted selection payload.
  Neither currently implements the complete-root/selected-Markdown dual payload.
- `DocumentSnapshot::preview_clipboard_payload` resolves on a private snapshot;
  this already avoids source and undo mutation and shares the edit resolver.
  Preserve its stale-address validation and conversion/edit atomicity.
- Wayland `Clipboard::send` ignores the requested MIME and sends plain text;
  `WaylandClient::write_to_clipboard` advertises `TEXT_MIME_TYPES` and a private
  process marker. GPUI string metadata alone cannot publish native `text/html`.
  Implement a real MIME-aware platform path, with bounded data and existing
  sanitization policy, then inspect actual `wl-paste --type text/html` and
  `text/plain` payloads for partial, reversed, emphasized, linked and nested
  selections. HTML root ownership must follow canonical document blocks.
- Replace the historical `--html-toolbar-check` fixture-36 harness with the
  seamless interaction contract; its old menu actions no longer exist.

This checkpoint does not claim native rich MIME support or completion of the
broader nested editing/clipboard work.

The same native fixture check also passes at 1280×1100 and 200% text zoom:
`layout-previews/html-seamless-find-200.*`. Search reveal, first-edit conversion,
exact Undo and source hashes pass with the new scaled geometry; final screenshot
inspected. Crusty validation `task_14a9308c2f8d214a` completed against
`ctx_9a71e4b23244`; final documentation validation follows this addendum.

## Native MIME implementation

The subsequent implementation replaces the remaining single-fragment plain-only
copy path with the source-verified preview resolver. For endpoints inside one
canonical HTML owner, `text/plain` is the selected Markdown, while `text/html`
contains that complete owner's sanitized HTML root. The private rich metadata
retains the selected Markdown, so same-process paste does not expand the root.
Selections crossing different canonical owners retain the existing mixed-preview
payload policy; broader A13 interoperability remains separate and open.

Clipboard export shares the bounded HTML allowlist but is distinct from the
renderer projection: safe link URLs, relative image references and authored IDs
are exported as HTML attributes, not private `data-mineral-*` descriptors.
Event handlers, active content and inline CSS are excluded. The latter cannot
rely on the app's denying resource provider once copied to another application.
No incoming foreign HTML or X11 clipboard behavior is claimed.

GPUI retains an explicit Linux HTML alternative independently of plain text and
private metadata. Wayland advertises it as `text/html` and dispatches requests
by MIME. Unknown formats no longer receive an unrelated plain representation.
Primary selection is unchanged. See the respective vendor patch notes.

This work also fixed a demonstrated conversion defect: a nested list immediately
after inline item text previously became literal `link- Nested` in its parent
paragraph. The converter now starts the nested marker on its own line before
parent indentation. A failing-before regression proves separate parent, nested
and sibling leaves survive conversion.

`/tmp/mineral-html-mime-export-check.log`: 790 tests pass, two existing ignored;
locked checks, formatting, strict Clippy, adapter suites and doctests pass.
The separate actual Wayland MIME dispatch unit test passes
(`/tmp/mineral-html-mime-platform-test.log`). Harness parser tests: 23 pass;
find-control oracle tests: four pass. Core tests cover reversed selections,
partial bold text, links, nested text, immutable source/revision/selection/undo,
and safe export without renderer descriptors or active attributes.

The old `--html-toolbar-check` and its implementation are replaced by
`--html-clipboard-check` with fixture 128. The native helper checks each advertised
format independently for eight forward/reverse selections, then exact saved
Markdown after first typing and byte-identical HTML after one Undo.

Native release `58120a6bf4dd9e1e4d2046d53d1653b6563090df41ca11569420732acc28d472`
passes all eight MIME cases, the exact first-edit save golden, and exact one-step
Undo at 900×900/100% and 1280×1100/200% text. Artifacts:
`layout-previews/html-mime-export-native{,-200}.{clipboard,source}.json` and PNGs.
The helper polls native MIME readiness; the first export run demonstrated one
transient empty HTML read followed immediately by the exact expected payload.
This is an eventual-delivery check, not a clipboard latency claim.

Both final screenshots were inspected. **Work remains active:** the final
`Other` sibling in fixture 128 is absent from the rendered list, although it is
present in original/restored source and every copied complete HTML root. This
is visual evidence of a rendering/geometry defect, not evidence of source loss.
Investigate the retained HTML raster, body/overflow height and per-leaf hit
geometry; qualify the entire list before declaring this interaction work done.
Also explicitly qualify mixed canonical-owner selection policy instead of
inferring it from these single-owner cases.

## Collapsed-margin raster clipping

The missing final item was reproduced directly through `html::render` and
minimized to `<ul><li>Other</li></ul>`. Its body origin is y=16 from collapsed
list margins, local body height is 24, and final text bounds end at y=37.284.
The image was only 24 pixels high. Initial and resized viewport geometry matched,
ruling out a resize-induced shift; local overflow also correctly reported 24.
The defect was mixing document-relative painting with body-local sizing.

Raster height now includes the body's absolute y offset, both during initial
measurement and the post-resize stability check. No arbitrary padding or CSS
margin suppression is used. The regression covers single/nested lists and zero/
48px authored margins at 200px and 600px widths; exact final-item caret geometry
must fit inside the resulting image. The failing-before command was
`cargo test --locked -p document-view nested_list_final_line_fits_the_raster`;
the final regression is `collapsed_top_margin_keeps_final_text_inside_the_raster`.
Temporary `[DEBUG-html-clip]` probes were removed.

`scripts/check.sh` passes 791 tests with two existing ignored, including locked
checks, formatting, strict Clippy, adapter suites and doctests
(`/tmp/mineral-html-clip-check.log`). The native helper now tests ten clipboard
cases including forward/reversed `Other`, then replaces that formerly clipped
item and checks exact saved Markdown and one-step source restoration.

The clipping defect is now qualified as fixed. Release SHA-256
`29ea60331c969f1d806256603562a53b856954c39648c2a11a7b9ad2eda36e94`
passes all ten actual Wayland clipboard payload cases and the final-item edit/
single-Undo golden at 900×900/100% and 1280×1100/200%. Both source hashes remain
exact. `layout-previews/html-clip-fixed{,-200}.png` were visually inspected:
`Other` is fully visible, including its bullet and complete glyphs, and the
following Markdown paragraph remains separate. JSON siblings record exact MIME,
edit/Undo and source results. This supersedes the preceding clipping blocker;
mixed canonical-owner clipboard policy remains to be qualified.

## Copy across canonical owners

Mixed HTML/Markdown selections now retain the private conversion's generated-ID
ranges as ephemeral ownership information. HTML export replaces each selected
owner's generated descendants with its complete sanitized root exactly once,
while retaining selected Markdown neighbors and enclosing containers. Each root
is normalized independently before assembly: a regression reproduced an unclosed
`<div><p>` swallowing following Markdown paragraphs in a combined parse.

Plain text contains selected Markdown. Selected table cells and definition
contents are flattened for this representation so source-preserving HTML
fallbacks cannot leak into text/plain. Private rich clipboard metadata retains
structural Markdown for application paste. One-owner selections spanning multiple
table leaves use the same plain-text policy. Copy still operates on a private
snapshot; source, revision, selection and content undo history remain unchanged.

Clipboard sanitization handles canonical alert containers, disabled task
checkboxes, and safe HTTP(S)/relative image sources; unsafe image sources leave
only alternate text. These export rules do not enable active controls or network
fetching in the preview. Core regressions cover forward/reversed multi-owner
selection, quotes, alerts, tasks, lists, table cells, multiple leaves in one root,
and incomplete authored tags. All 123 document-core tests pass.

The native helper now also accepts fixture 44. Four exact MIME cases cover both
selection directions, two HTML owners with intervening Markdown, a trailing
Markdown endpoint, and a selection containing the document heading. Each case
checks source preservation and an empty content undo history; the final edit
checks exact saved Markdown and byte-identical original HTML after one Undo.

Final qualification: `scripts/check.sh` passes 795 tests, with two existing
ignored tests, plus formatting, locked checks, strict Clippy, adapter suites and
doctests (`/tmp/mineral-html-cross-final-check.log`). Harness parser tests: 23
pass; find-control oracle tests: four pass. Python compilation and diff whitespace
checks pass.

Release SHA-256
`3caec16181a6917e24a56476fc8d94e60033f618d10b249e988fb53c78f82b35`
passes all four cross-owner MIME cases at 900×1000/100% and 1280×1100/200%, and
all ten single-owner cases at 900×1000/100%. All three runs pass exact first-edit
save, one-step Undo and unchanged source hashes. Artifacts are
`layout-previews/html-cross-final{,-200}.{clipboard,source}.json`,
`layout-previews/html-single-final.{clipboard,source}.json`, and sibling PNGs.
All three screenshots were inspected: both roots and intervening prose remain
separate, the final nested-list sibling is visible, and obsolete HTML action
buttons are absent.

This completes the seamless HTML selection/copy/first-edit interaction contract
and supersedes the preceding mixed-owner qualification gap. Native MIME evidence
is for Wayland; incoming HTML paste and X11 clipboard interoperability remain
under the separate A13 work item, not this completion claim.
