# A source-backed review

The explanation and its diagram describe the same route. Both remain readable when there is room to place them together.

## Input and decision

The input enters a processing stage before reaching a decision. The approved branch produces Output A; the revision branch produces Output B. Follow the arrowheads and their labels to distinguish the two outcomes. This explanation and the figure preserve the same authored sequence without relying on color.

```mermaid
flowchart LR
accTitle: Review route
accDescr: Process the input, then choose the approved or revision output.
A[Input] --> B[Process]
B --> C{Decision}
C -->|Approved| D[Output A]
C -->|Revise| E[Output B]
```

## Following work

The next section stays below the complete explanation and figure. Editing the diagram exposes its original source; the surrounding document remains intact.
