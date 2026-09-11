# A shared language

Terms and their explanations form an open glossary. The labels share a measured rail; rich descriptions keep their original reading order and stack when space tightens.

## Layout vocabulary

Canvas
: The available document surface after navigation and outer margins have been reserved.

Reading measure
: A comfortable text width, measured with the loaded typeface rather than estimated from a fixed character count.

Stable identity
: The source-owned identity of a block. **Changing its placement does not change its content.**
: Selection, links and undo continue to address that same block.

## Rich descriptions

Prepared geometry
: Text is shaped before the reader scrolls. A description can contain supporting paragraphs and a code example.

    ```rust
    let width = canvas.available_width();
    let rows = grammar.measure(document, width);
    ```

    The code retains its whitespace, language label and Copy action.

Relationships
: Preserve the meaning of each group:

    - Terms name concepts.
    - Descriptions explain them.
    - Layout never invents a rank or completion state.

## Labels that need room

A deliberately long term that should not squeeze its description into a narrow corridor
: This definition stacks beneath its label at the full reading measure. All text remains visible and editable.
