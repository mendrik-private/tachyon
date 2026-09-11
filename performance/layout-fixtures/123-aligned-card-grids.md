# Aligned collections

### Document model and interfaces

Use stable node IDs, typed block nodes, rich inline runs, and rope-backed editable text. Table cells contain block sequences.

The central interfaces are:

- `DocumentSnapshot`: immutable document revision with shared unchanged storage.
- `DocumentPosition`: node ID, text offset, and affinity.
- `Selection`: text range or rectangular table range.
- `EditCommand -> TransactionResult`: document changes, transformed selection, inverse operations, and dirty node IDs.
- `LayoutIndex`: node/fragment geometry, cumulative heights, and document-position mapping.
- `SaveSnapshot`: serialized revision plus its expected on-disk identity.

## Completed review

- [x] Read the project overview.
- [x] Review the current design.
- [x] Explore an example document.
- [x] Compare the recorded decisions.
- [x] Talk through an open question.

## Shared columns

- Read the project overview.
- Review the current design.
- Explore an example document.
- Compare the recorded decisions.
- Talk through an open question.

## Seven useful perspectives

- An observation from the field.
- An account from an interview.
- A finding from published work.
- A sketch of a possible approach.
- A note from a project review.
- A question for further research.
- A decision with its explanation.

