//! Source-order note identities. Visual columns never determine numbering.
use document_core::{BlockNode, DocumentPosition, InlineStyle, NodeId};
use rustc_hash::FxHashMap;

use crate::TextProjection;

pub(crate) type Numbering = std::sync::Arc<[(String, Option<usize>)]>;

#[derive(Clone, Debug)]
pub(crate) struct Note {
    pub label: String,
    pub number: Option<usize>,
    pub definition: NodeId,
    pub body: NodeId,
    pub references: Vec<DocumentPosition>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Index {
    pub notes: Vec<Note>,
    pub numbering: Numbering,
    labels: FxHashMap<String, usize>,
    definitions: FxHashMap<NodeId, usize>,
}

impl Index {
    pub fn build(projection: &TextProjection) -> Self {
        let mut index = Self::default();
        for segment in projection.segments() {
            let Some(id) = segment.context.footnote else {
                continue;
            };
            if index.definitions.contains_key(&id) {
                continue;
            }
            let Some(BlockNode::FootnoteDefinition { label, .. }) = projection.block(id) else {
                continue;
            };
            let slot = index.notes.len();
            index.definitions.insert(id, slot);
            index.labels.entry(label.clone()).or_insert(slot);
            index.notes.push(Note {
                label: label.clone(),
                number: None,
                definition: id,
                body: segment.node_id,
                references: Vec::new(),
            });
        }
        let mut number = 0;
        for segment in projection.segments() {
            let Some(text) = projection.block(segment.node_id).and_then(BlockNode::text) else {
                continue;
            };
            for run in text.runs() {
                let Some(label) = run.styles.iter().find_map(|style| match style {
                    InlineStyle::FootnoteReference(label) => Some(label),
                    _ => None,
                }) else {
                    continue;
                };
                let Some(slot) = index.labels.get(label) else {
                    continue;
                };
                let note = &mut index.notes[*slot];
                if note.number.is_none() {
                    number += 1;
                    note.number = Some(number);
                }
                note.references.push(DocumentPosition::new(
                    segment.node_id,
                    run.range.start,
                    document_core::Affinity::Downstream,
                ));
            }
        }
        index.numbering = index
            .notes
            .iter()
            .map(|note| (note.label.clone(), note.number))
            .collect();
        index
    }

    pub fn label(&self, label: &str) -> Option<&Note> {
        self.labels.get(label).map(|slot| &self.notes[*slot])
    }

    pub fn definition(&self, id: NodeId) -> Option<&Note> {
        self.definitions.get(&id).map(|slot| &self.notes[*slot])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_number_by_source_not_definition_order_and_keep_repetitions() {
        let source = "Before[^later] and after[^first]; again[^later].\n\n[^first]: First definition.\n\n[^later]: Later definition.\n\n    A second paragraph.\n";
        let doc = document_core::Document::from_markdown(source).unwrap();
        let projection = TextProjection::from_snapshot(&doc.snapshot());
        let first = projection.footnotes.label("first").unwrap();
        let later = projection.footnotes.label("later").unwrap();
        assert_eq!(first.number, Some(2));
        assert_eq!(later.number, Some(1));
        assert_eq!(later.references.len(), 2);
        assert!(later.references[0].text_offset < later.references[1].text_offset);
        assert_eq!(
            projection
                .segments()
                .iter()
                .filter(|s| s.context.footnote == Some(later.definition))
                .count(),
            2
        );
        assert_eq!(doc.snapshot().serialize().unwrap(), source);
    }

    #[test]
    fn adjacent_references_to_one_note_have_two_source_anchors() {
        let doc =
            document_core::Document::from_markdown("Evidence[^n][^n].\n\n[^n]: Source.\n").unwrap();
        let projection = TextProjection::from_snapshot(&doc.snapshot());
        let note = projection.footnotes.label("n").unwrap();
        assert_eq!(note.references.len(), 2);
        assert_eq!(
            note.references[1].text_offset - note.references[0].text_offset,
            4
        );
    }
}
