## 1. Verdict and scope

The three-crate foundation is reasonable and should be retained. The immediate problem is incomplete integration between the document model, serialization, editing, geometry, and application lifecycle. Visual polish alone will not make this editor reliable.

**Fix save/reopen correctness and selection invariants first.** The audit reproduced paragraph boundaries disappearing after save, literal prose changing into Markdown syntax, table formatting being discarded, and a caret pointing into a deleted table row. The native UI also edits the wrong table column and paints text across cell boundaries. These are release blockers.

The code compiles and its existing tests pass. That does not contradict these findings: most tests exercise helpers or selected happy paths rather than complete input → transaction → save → reopen workflows.

### Constraints to preserve

The following section starts after the complete argument.
