# Native document workspace

A continuously rendered Markdown surface for calm long-form writing and dense technical documents.

## 1. Product contract

The editing surface is the document. Clicking places the caret, selecting text reveals formatting tools, and source order remains stable while the canvas adapts to its content.

The automatic planner may place a compact independent group beside this introduction when both fit at a comfortable measure.

- [x] Rich content inside table cells
- [x] Byte-preserving autosave
- [x] Files and Outline; no minimap
- [x] Automatic layout with stable source order
- [x] Light theme and native controls

## 2. Visual and component system

Matched sibling sections may share one row when their complete content remains readable.

### Typography

| Role | Face | Size / leading |
| --- | --- | ---: |
| Body | Spline Sans | 18 / 28.8 |
| H1 | Fraunces 600 | 44 / 50 |
| H2 | Fraunces 600 | 28 / 34 |
| Code | Spline Sans Mono | 15 / 22.5 |

### Palette

| Role | Value |
| --- | --- |
| Page / navigation | `#FCFBF8` / `#F3F2ED` |
| Text / secondary | `#1B2430` / `#59636F` |
| Accent / selection | `#256F50` / `#DCEBE1` |
| Rules / errors | `#DEDFD7` / `#9E4B3F` |

### Window anatomy

| Region | Behavior |
| --- | --- |
| Files | Resizable navigation and local documents |
| Canvas | Centered prose with elastic components |
| Margin | Optional notes fold into the flow |

## 3. Architecture and persistence

Three complete sibling sections form a compact feature row when their structure, height, and available width agree.

### Document core

Stable nodes, source-preserving import, transactions, selection, undo, and serialization.

### Document view

Measured shaping, automatic arrangements, hit testing, virtualization, tables, HTML, and formulas.

### Application

Native shell, file navigation, autosave, recovery, image loading, and performance instrumentation.

## 4. Explanation and example

The explanation belongs with the short example when both fit; on a narrow viewport they stack without changing the document.

```rust
let plan = LayoutPlan::measure(document, viewport);
assert!(plan.preserves_source_order());
```

## 5. Final reading section

Ordinary prose returns to a comfortable measure after the bounded compositions. Editing, keyboard traversal, selection, and autosave continue to operate on the same canonical document tree.
