use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use document_core::{
    Document, DocumentError, DocumentSnapshot, EditCommand, TextSelection, TransactionResult,
};

/// UI-thread ownership for one document, shared by every view of the same file.
///
/// GPUI entities are single-threaded, so `Rc<RefCell<_>>` keeps mutation and undo
/// centralized without adding a misleading cross-thread synchronization boundary.
struct SessionState {
    document: RefCell<Document>,
    generation: Cell<u64>,
}

#[derive(Clone)]
pub struct SharedDocumentSession(Rc<SessionState>);

impl SharedDocumentSession {
    #[must_use]
    pub fn new(document: Document) -> Self {
        Self(Rc::new(SessionState {
            document: RefCell::new(document),
            generation: Cell::new(0),
        }))
    }

    #[must_use]
    pub fn snapshot(&self) -> DocumentSnapshot {
        self.0.document.borrow().snapshot()
    }

    #[must_use]
    pub fn generation(&self) -> u64 {
        self.0.generation.get()
    }

    pub fn replace(&self, document: Document) {
        *self.0.document.borrow_mut() = document;
        self.bump_generation();
    }

    pub fn apply(&self, command: EditCommand) -> Result<TransactionResult, DocumentError> {
        let before = self.snapshot().revision();
        let result = self.0.document.borrow_mut().apply(command)?;
        if result.revision != before {
            self.bump_generation();
        }
        Ok(result)
    }

    pub fn undo(&self) -> Result<DocumentSnapshot, DocumentError> {
        let snapshot = self.0.document.borrow_mut().undo()?;
        self.bump_generation();
        Ok(snapshot)
    }

    pub fn redo(&self) -> Result<DocumentSnapshot, DocumentError> {
        let snapshot = self.0.document.borrow_mut().redo()?;
        self.bump_generation();
        Ok(snapshot)
    }

    #[must_use]
    pub fn composition_active(&self) -> bool {
        self.0.document.borrow().composition_active()
    }

    pub fn begin_composition(&self, range: TextSelection) -> Result<(), DocumentError> {
        self.0.document.borrow_mut().begin_composition(range)
    }

    pub fn update_composition(&self, text: String) -> Result<DocumentSnapshot, DocumentError> {
        let snapshot = self.0.document.borrow_mut().update_composition(text)?;
        self.bump_generation();
        Ok(snapshot)
    }

    pub fn commit_composition(&self) -> Result<DocumentSnapshot, DocumentError> {
        self.0.document.borrow_mut().commit_composition()
    }

    #[must_use]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    #[must_use]
    pub fn strong_count(&self) -> usize {
        Rc::strong_count(&self.0)
    }

    pub(crate) fn with_document<T>(&self, read: impl FnOnce(&Document) -> T) -> T {
        read(&self.0.document.borrow())
    }

    fn bump_generation(&self) {
        self.0
            .generation
            .set(self.0.generation.get().wrapping_add(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use document_core::{Affinity, DocumentPosition, Selection};

    #[test]
    fn clones_share_content_and_undo_history() {
        let session = SharedDocumentSession::new(Document::from_markdown("one").expect("document"));
        let second_view = session.clone();
        assert!(session.ptr_eq(&second_view));
        let node_id = session.snapshot().blocks().get(0).expect("paragraph").id();
        session
            .apply(EditCommand::SetSelection(Selection::Text(
                TextSelection::caret(DocumentPosition::new(node_id, 3, Affinity::Downstream)),
            )))
            .expect("caret");
        session
            .apply(EditCommand::ReplaceSelection {
                text: " two".into(),
                typing: false,
            })
            .expect("edit");
        assert_eq!(
            second_view.snapshot().serialize().expect("serialize"),
            "one two"
        );
        second_view.undo().expect("shared undo");
        assert_eq!(session.snapshot().serialize().expect("serialize"), "one");
    }
}
