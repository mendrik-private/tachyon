use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    BlockNode, BlockSequence, BlockStyle, DocumentError, DocumentPosition, EditCommand,
    FormatState, InlineFormat, InsertBlockKind, InverseOperation, NodeId, PositionError, Revision,
    SaveSnapshot, Selection, SourceIdentity, SourceSpine, TYPING_GROUP_TIMEOUT_MS, Table,
    TextSelection, TransactionResult, command::caret_after,
};

mod preview;

#[derive(Clone, Debug)]
struct SnapshotState {
    revision: Revision,
    blocks: BlockSequence,
    selection: Selection,
    dirty_nodes: BTreeSet<NodeId>,
    structure_changed: bool,
    source: SourceSpine,
    next_node_id: u64,
    node_revisions: Arc<HashMap<NodeId, Revision>>,
    /// Import/repair-only caret host, not authored content. Pointer identity
    /// distinguishes the untouched host from a paragraph the user has edited.
    transient_caret: Option<Arc<BlockNode>>,
}

#[derive(Clone, Debug)]
pub struct DocumentSnapshot(Arc<SnapshotState>);

impl DocumentSnapshot {
    #[must_use]
    pub fn revision(&self) -> Revision {
        self.0.revision
    }

    #[must_use]
    pub fn blocks(&self) -> &BlockSequence {
        &self.0.blocks
    }

    #[must_use]
    pub fn selection(&self) -> &Selection {
        &self.0.selection
    }

    #[must_use]
    pub fn source_spine(&self) -> &SourceSpine {
        &self.0.source
    }

    pub(crate) fn is_transient_caret(&self, block: &Arc<BlockNode>) -> bool {
        self.0
            .transient_caret
            .as_ref()
            .is_some_and(|caret| Arc::ptr_eq(caret, block))
    }

    #[must_use]
    pub fn dirty_node_ids(&self) -> &BTreeSet<NodeId> {
        &self.0.dirty_nodes
    }

    #[must_use]
    pub fn node_revision(&self, node_id: NodeId) -> Option<Revision> {
        self.0.blocks.contains_node(node_id).then(|| {
            self.0
                .node_revisions
                .get(&node_id)
                .copied()
                .unwrap_or_default()
        })
    }

    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&BlockNode> {
        find_node(&self.0.blocks, id)
    }

    #[must_use]
    pub fn table_cell_containing(&self, node_id: NodeId) -> Option<(NodeId, usize, usize)> {
        table_cell_containing(&self.0.blocks, node_id)
    }

    #[must_use]
    pub fn list_item_containing(&self, node_id: NodeId) -> Option<(NodeId, NodeId, usize)> {
        list_item_containing(&self.0.blocks, node_id)
    }

    pub fn serialize(&self) -> Result<String, DocumentError> {
        crate::markdown::serialize(self)
    }

    /// Copy a rendered range without changing this snapshot, its source,
    /// selection or undo history. Uses the same resolver as conversion-on-edit.
    pub fn preview_clipboard_payload(
        &self,
        selection: &crate::PreviewSelection,
    ) -> Result<Option<crate::ClipboardPayload>, DocumentError> {
        let mut state = (*self.0).clone();
        preview::resolve(&mut state, selection)?;
        DocumentSnapshot(Arc::new(state)).clipboard_payload()
    }

    pub fn save_snapshot(
        &self,
        expected_identity: Option<SourceIdentity>,
    ) -> Result<SaveSnapshot, DocumentError> {
        Ok(SaveSnapshot {
            revision: self.revision(),
            bytes: self.serialize()?.into_bytes().into(),
            expected_identity,
        })
    }

    #[must_use]
    pub fn validates_position(&self, position: DocumentPosition) -> bool {
        self.node(position.node_id)
            .and_then(BlockNode::text)
            .is_some_and(|text| {
                position.text_offset <= text.len()
                    && text.as_string().is_char_boundary(position.text_offset)
            })
    }
}

fn table_cell_containing(
    blocks: &BlockSequence,
    node_id: NodeId,
) -> Option<(NodeId, usize, usize)> {
    for block in blocks {
        match block.as_ref() {
            BlockNode::Table(table) => {
                for (row_index, row) in table.rows.iter().enumerate() {
                    for (column_index, cell) in row.cells.iter().enumerate() {
                        if cell.blocks.iter().any(|block| {
                            block.id() == node_id || block_contains_node(block, node_id)
                        }) {
                            return Some((table.id, row_index, column_index));
                        }
                    }
                }
            }
            BlockNode::List(list) => {
                for item in list.items.iter() {
                    if let Some(found) = table_cell_containing(&item.blocks, node_id) {
                        return Some(found);
                    }
                }
            }
            BlockNode::BlockQuote { blocks, .. }
            | BlockNode::Alert { blocks, .. }
            | BlockNode::FootnoteDefinition { blocks, .. } => {
                if let Some(found) = table_cell_containing(blocks, node_id) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

fn list_item_containing(
    blocks: &BlockSequence,
    node_id: NodeId,
) -> Option<(NodeId, NodeId, usize)> {
    for block in blocks {
        match block.as_ref() {
            BlockNode::List(list) => {
                for (index, item) in list.items.iter().enumerate() {
                    if item
                        .blocks
                        .iter()
                        .any(|block| block.id() == node_id || block_contains_node(block, node_id))
                    {
                        if let Some(nested) = list_item_containing(&item.blocks, node_id) {
                            return Some(nested);
                        }
                        return Some((list.id, item.id, index));
                    }
                }
            }
            BlockNode::BlockQuote { blocks, .. }
            | BlockNode::Alert { blocks, .. }
            | BlockNode::FootnoteDefinition { blocks, .. } => {
                if let Some(found) = list_item_containing(blocks, node_id) {
                    return Some(found);
                }
            }
            BlockNode::Table(table) => {
                for row in table.rows.iter() {
                    for cell in row.cells.iter() {
                        if let Some(found) = list_item_containing(&cell.blocks, node_id) {
                            return Some(found);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    None
}

#[derive(Clone, Debug)]
pub struct RevisionTagged<T> {
    pub revision: Revision,
    pub value: T,
}

#[derive(Clone, Debug)]
struct HistoryEntry {
    before: DocumentSnapshot,
    after: DocumentSnapshot,
    after_selection: Selection,
}

#[derive(Clone, Debug)]
struct TypingGroup {
    node_id: NodeId,
    last_edit: Instant,
    history_index: usize,
}

#[derive(Clone, Debug)]
struct Composition {
    before: DocumentSnapshot,
    /// Provisional edits restart here. Usually identical to `before`; HTML
    /// composition uses a converted baseline without publishing a content edit.
    baseline: DocumentSnapshot,
    range: TextSelection,
}

#[derive(Clone, Copy, Debug)]
enum SelectionMutation {
    None,
    ExplicitTextSelection,
    InsertRow {
        table_id: NodeId,
        index: usize,
    },
    DeleteRow {
        table_id: NodeId,
        index: usize,
    },
    MoveRow {
        table_id: NodeId,
        from: usize,
        to: usize,
    },
    InsertColumn {
        table_id: NodeId,
        index: usize,
    },
    DeleteColumn {
        table_id: NodeId,
        index: usize,
    },
    MoveColumn {
        table_id: NodeId,
        from: usize,
        to: usize,
    },
}

impl SelectionMutation {
    fn from_command(command: &EditCommand) -> Self {
        match command {
            EditCommand::ReplaceText {
                selection_after: Some(_),
                ..
            } => Self::ExplicitTextSelection,
            EditCommand::InsertTableRow { table_id, index } => Self::InsertRow {
                table_id: *table_id,
                index: *index,
            },
            EditCommand::DeleteTableRow { table_id, index } => Self::DeleteRow {
                table_id: *table_id,
                index: *index,
            },
            EditCommand::MoveTableRow { table_id, from, to } => Self::MoveRow {
                table_id: *table_id,
                from: *from,
                to: *to,
            },
            EditCommand::InsertTableColumn { table_id, index } => Self::InsertColumn {
                table_id: *table_id,
                index: *index,
            },
            EditCommand::DeleteTableColumn { table_id, index } => Self::DeleteColumn {
                table_id: *table_id,
                index: *index,
            },
            EditCommand::MoveTableColumn { table_id, from, to } => Self::MoveColumn {
                table_id: *table_id,
                from: *from,
                to: *to,
            },
            _ => Self::None,
        }
    }
}

#[derive(Debug)]
pub struct Document {
    current: DocumentSnapshot,
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    typing_group: Option<TypingGroup>,
    composition: Option<Composition>,
    revision_counter: Revision,
}

impl Document {
    pub fn from_markdown(source: impl Into<Arc<str>>) -> Result<Self, DocumentError> {
        let imported = crate::markdown::import(source.into())?;
        Ok(Self {
            current: imported,
            undo: Vec::new(),
            redo: Vec::new(),
            typing_group: None,
            composition: None,
            revision_counter: Revision::default(),
        })
    }

    pub fn from_html(source: &str) -> Result<Self, DocumentError> {
        let markdown = crate::html::html_fragment_to_markdown(source)?;
        Self::from_markdown(Arc::<str>::from(markdown))
    }

    #[must_use]
    pub fn empty() -> Self {
        Self::from_markdown(Arc::<str>::from(""))
            .expect("an empty CommonMark document is always valid")
    }

    #[must_use]
    pub fn snapshot(&self) -> DocumentSnapshot {
        self.current.clone()
    }

    #[must_use]
    pub fn composition_active(&self) -> bool {
        self.composition.is_some()
    }

    pub fn apply(&mut self, command: EditCommand) -> Result<TransactionResult, DocumentError> {
        self.apply_at(command, Instant::now())
    }

    fn apply_at(
        &mut self,
        command: EditCommand,
        now: Instant,
    ) -> Result<TransactionResult, DocumentError> {
        if command.is_selection_only() {
            let EditCommand::SetSelection(selection) = command else {
                unreachable!();
            };
            validate_selection(&self.current, &selection)?;
            let mut state = (*self.current.0).clone();
            state.selection = selection;
            self.current = DocumentSnapshot(Arc::new(state));
            self.typing_group = None;
            return Ok(TransactionResult::unchanged(self.current.clone()));
        }

        if self.composition.is_some() {
            return Err(DocumentError::CompositionAlreadyActive);
        }

        let before = self.current.clone();
        let before_selection = before.selection().clone();
        let mut text_changed_node = command.localized_text_node(&before_selection).filter(|id| {
            matches!(
                before.node(*id),
                Some(BlockNode::Paragraph(_) | BlockNode::Heading(_) | BlockNode::CodeBlock(_))
            )
        });
        let mut localized_text_node = command
            .localized_text_node(&before_selection)
            .filter(|node_id| before.blocks().top_index_of(*node_id).is_some());
        let typing_node = command.typing_node().or_else(|| match &command {
            EditCommand::ReplaceSelection { typing: true, .. } => {
                let Selection::Text(selection) = before.selection() else {
                    return None;
                };
                (selection.anchor.node_id == selection.head.node_id)
                    .then_some(selection.anchor.node_id)
            }
            _ => None,
        });
        let selection_mutation = SelectionMutation::from_command(&command);
        let mut state = (*before.0).clone();
        let mut preview_converted = false;
        let command = match command {
            EditCommand::EditPreviewSelection { selection, edit } => {
                preview_converted = preview::resolve(&mut state, &selection)?;
                if matches!(&edit, crate::HtmlTextEdit::Replace(text) if text.is_empty())
                    && matches!(&state.selection, Selection::Text(range) if range.is_caret())
                {
                    return Ok(TransactionResult::unchanged(before));
                }
                edit.into_command()
            }
            command => command,
        };
        let revision = self.allocate_revision();
        state.revision = revision;
        let changed = apply_command(&mut state, command)? || preview_converted;
        if !changed {
            return Ok(TransactionResult::unchanged(before));
        }
        reconcile_selection(&before, &mut state, selection_mutation)?;
        if retire_transient_caret(&mut state) {
            localized_text_node = None;
            text_changed_node = None;
        }
        let transaction_dirty = localized_text_node.map_or_else(
            || changed_node_ids(&before.0.blocks, &state.blocks),
            |node_id| BTreeSet::from([node_id]),
        );
        state.dirty_nodes.extend(&transaction_dirty);
        let node_revisions = Arc::make_mut(&mut state.node_revisions);
        for node_id in &transaction_dirty {
            node_revisions.insert(*node_id, revision);
        }
        if localized_text_node.is_none() {
            validate_tree(&state.blocks)?;
        }
        let after = DocumentSnapshot(Arc::new(state));
        let after_selection = after.selection().clone();
        self.current = after.clone();
        self.redo.clear();

        let can_group = typing_node.is_some_and(|node_id| {
            self.typing_group.as_ref().is_some_and(|group| {
                group.node_id == node_id
                    && now.saturating_duration_since(group.last_edit)
                        <= Duration::from_millis(TYPING_GROUP_TIMEOUT_MS)
                    && group.history_index + 1 == self.undo.len()
            })
        });
        if can_group {
            if let Some(entry) = self.undo.last_mut() {
                entry.after = after.clone();
                entry.after_selection = after_selection.clone();
            }
            if let Some(group) = &mut self.typing_group {
                group.last_edit = now;
            }
        } else {
            self.undo.push(HistoryEntry {
                before: before.clone(),
                after: after.clone(),
                after_selection: after_selection.clone(),
            });
            self.typing_group = typing_node.map(|node_id| TypingGroup {
                node_id,
                last_edit: now,
                history_index: self.undo.len() - 1,
            });
        }

        Ok(TransactionResult {
            revision: after.revision(),
            selection: after_selection,
            dirty_node_ids: transaction_dirty,
            text_changed_node,
            inverse_operations: vec![InverseOperation::Restore {
                snapshot: before,
                selection: before_selection,
            }],
            snapshot: after,
        })
    }

    pub fn undo(&mut self) -> Result<DocumentSnapshot, DocumentError> {
        if self.composition.is_some() {
            return Err(DocumentError::CompositionAlreadyActive);
        }
        let entry = self.undo.pop().ok_or(DocumentError::NothingToUndo)?;
        self.current = self.restore_with_fresh_revision(&entry.before);
        self.redo.push(entry);
        self.typing_group = None;
        Ok(self.current.clone())
    }

    pub fn redo(&mut self) -> Result<DocumentSnapshot, DocumentError> {
        if self.composition.is_some() {
            return Err(DocumentError::CompositionAlreadyActive);
        }
        let entry = self.redo.pop().ok_or(DocumentError::NothingToRedo)?;
        self.current = self.restore_with_fresh_revision(&entry.after);
        self.undo.push(entry);
        self.typing_group = None;
        Ok(self.current.clone())
    }

    pub fn begin_composition(&mut self, range: TextSelection) -> Result<(), DocumentError> {
        if self.composition.is_some() {
            return Err(DocumentError::CompositionAlreadyActive);
        }
        validate_selection(&self.current, &Selection::Text(range.clone()))?;
        validate_composition_range(&self.current.0, &range)?;
        self.typing_group = None;
        self.composition = Some(Composition {
            before: self.current.clone(),
            baseline: self.current.clone(),
            range,
        });
        Ok(())
    }

    /// Prepare composition in verified HTML text. Conversion remains
    /// provisional: begin leaves the current snapshot untouched, updates use
    /// the converted baseline, and cancellation/undo restore the original HTML.
    pub fn begin_html_composition(
        &mut self,
        node_id: NodeId,
        expected_source: &str,
        anchor: crate::HtmlTextPosition,
        head: crate::HtmlTextPosition,
    ) -> Result<(), DocumentError> {
        let endpoint = |position| crate::PreviewPosition::Html {
            node_id,
            expected_source: Arc::from(expected_source),
            position,
        };
        self.begin_preview_composition(&crate::PreviewSelection {
            revision: self.current.revision(),
            anchor: endpoint(anchor),
            head: endpoint(head),
        })
    }

    /// Prepare a source-verified preview selection for provisional IME edits.
    /// All conversions and replacement validation happen on private snapshots.
    /// Until update, neither the current source nor its revision changes. Each
    /// update starts from the same baseline; cancel and one undo restore the
    /// original HTML and Markdown. Unsupported nested replacements fail before
    /// composition ownership is acquired.
    pub fn begin_preview_composition(
        &mut self,
        selection: &crate::PreviewSelection,
    ) -> Result<(), DocumentError> {
        if self.composition.is_some() {
            return Err(DocumentError::CompositionAlreadyActive);
        }
        let mut state = (*self.current.0).clone();
        preview::resolve(&mut state, selection)?;
        let Selection::Text(range) = state.selection.clone() else {
            return Err(DocumentError::TableSelectionForTextCommand);
        };
        validate_composition_range(&state, &range)?;
        self.typing_group = None;
        self.composition = Some(Composition {
            before: self.current.clone(),
            baseline: DocumentSnapshot(Arc::new(state)),
            range,
        });
        Ok(())
    }

    pub fn update_composition(&mut self, text: String) -> Result<DocumentSnapshot, DocumentError> {
        let composition = self
            .composition
            .as_ref()
            .ok_or(DocumentError::NoComposition)?
            .clone();
        let mut state = (*composition.baseline.0).clone();
        let revision = self.allocate_revision();
        state.revision = revision;
        replace_text_selection(&mut state, composition.range, text, false)?;
        retire_transient_caret(&mut state);
        let transaction_dirty = changed_node_ids(&composition.before.0.blocks, &state.blocks);
        state.dirty_nodes.extend(&transaction_dirty);
        let node_revisions = Arc::make_mut(&mut state.node_revisions);
        for node_id in transaction_dirty {
            node_revisions.insert(node_id, revision);
        }
        self.current = DocumentSnapshot(Arc::new(state));
        Ok(self.current.clone())
    }

    pub fn commit_composition(&mut self) -> Result<DocumentSnapshot, DocumentError> {
        let composition = self
            .composition
            .take()
            .ok_or(DocumentError::NoComposition)?;
        let after = self.current.clone();
        if after.revision() == composition.before.revision() {
            return Ok(after);
        }
        self.undo.push(HistoryEntry {
            before: composition.before,
            after: after.clone(),
            after_selection: after.selection().clone(),
        });
        self.redo.clear();
        Ok(after)
    }

    pub fn cancel_composition(&mut self) -> Result<DocumentSnapshot, DocumentError> {
        let composition = self
            .composition
            .take()
            .ok_or(DocumentError::NoComposition)?;
        self.current = self.restore_with_fresh_revision(&composition.before);
        Ok(self.current.clone())
    }

    pub fn accept_worker_result<T>(&self, result: RevisionTagged<T>) -> Result<T, DocumentError> {
        if result.revision != self.current.revision() {
            return Err(DocumentError::StaleWorkerResult {
                result: result.revision,
                current: self.current.revision(),
            });
        }
        Ok(result.value)
    }

    pub fn format_state(
        &self,
        node_id: NodeId,
        range: std::ops::Range<usize>,
        format: &InlineFormat,
    ) -> Result<FormatState, DocumentError> {
        let text = self
            .current
            .node(node_id)
            .ok_or(PositionError::UnknownNode(node_id))?
            .text()
            .ok_or(PositionError::NotText(node_id))?;
        text.validate_range(node_id, &range)?;
        let (covered, total) = text.style_coverage(&range, &format.as_style());
        Ok(if covered == 0 {
            FormatState::Off
        } else if covered == total {
            FormatState::On
        } else {
            FormatState::Mixed
        })
    }

    fn allocate_revision(&mut self) -> Revision {
        self.revision_counter = self.revision_counter.next();
        self.revision_counter
    }

    fn restore_with_fresh_revision(&mut self, source: &DocumentSnapshot) -> DocumentSnapshot {
        let revision = self.allocate_revision();
        let mut state = (*source.0).clone();
        state.revision = revision;
        let changed = changed_node_ids(&self.current.0.blocks, &state.blocks);
        let revisions = Arc::make_mut(&mut state.node_revisions);
        for node_id in changed {
            revisions.insert(node_id, revision);
        }
        DocumentSnapshot(Arc::new(state))
    }
}

/// Retire only an untouched, unselected caret scaffold once real editable
/// content exists. A mutated host becomes authored content, even if empty.
/// Run after resolving the complete selection, never midway through converting
/// its endpoints: the scaffold itself can be a valid range endpoint.
fn retire_transient_caret(state: &mut SnapshotState) -> bool {
    let Some(caret) = state.transient_caret.as_ref() else {
        return false;
    };
    let id = caret.id();
    let index = state.blocks.top_index_of(id);
    if !index
        .and_then(|index| state.blocks.get(index))
        .is_some_and(|current| Arc::ptr_eq(caret, current))
    {
        state.transient_caret = None;
        return false;
    }
    if matches!(&state.selection, Selection::Text(selection)
        if selection.anchor.node_id == id || selection.head.node_id == id)
        || !state
            .blocks
            .iter()
            .any(|block| block.id() != id && first_editable_in_block(block).is_some())
    {
        return false;
    }
    let mut blocks = state.blocks.to_vec();
    blocks.remove(index.expect("verified top-level caret host"));
    state.blocks = BlockSequence::new(blocks);
    state.transient_caret = None;
    state.structure_changed = true;
    true
}

fn convert_html_to_markdown(
    state: &mut SnapshotState,
    node_id: NodeId,
    target: Option<(&str, crate::HtmlTextPosition, crate::HtmlTextPosition)>,
) -> Result<TextSelection, DocumentError> {
    let Some(BlockNode::PreservedSource { source, .. }) = find_node(&state.blocks, node_id) else {
        return Err(DocumentError::Html(
            "Only preserved HTML can be converted".into(),
        ));
    };
    if target.is_some_and(|(expected, _, _)| source.as_ref() != expected) {
        return Err(DocumentError::Html(
            "The HTML changed; choose the text again".into(),
        ));
    }
    let markdown = crate::html::editable_html_markdown(source).ok_or_else(|| {
        DocumentError::Html(
            "This HTML needs its original structure; conversion is unavailable".into(),
        )
    })?;
    let imported = crate::markdown::import(Arc::<str>::from(markdown))?;
    let replacement = remap_sequence_ids(imported.blocks(), &mut state.next_node_id);
    let selection = if let Some((_, anchor, head)) = target {
        let leaves = crate::html::conversion_text_blocks(&replacement);
        let position = |target: crate::HtmlTextPosition, affinity| {
            let block = leaves
                .get(target.text_node)
                .ok_or_else(|| DocumentError::Html("The HTML text target is unavailable".into()))?;
            let text = block
                .text()
                .ok_or_else(|| DocumentError::Html("No editable text at this target".into()))?;
            text.validate_range(block.id(), &(target.byte_offset..target.byte_offset))?;
            Ok::<_, DocumentError>(DocumentPosition::new(
                block.id(),
                target.byte_offset,
                affinity,
            ))
        };
        let anchor_position = position(anchor, crate::Affinity::Downstream)?;
        if anchor == head {
            TextSelection::caret(anchor_position)
        } else {
            TextSelection {
                anchor: anchor_position,
                head: position(head, crate::Affinity::Upstream)?,
            }
        }
    } else {
        TextSelection::caret(
            first_editable_position_in_sequence(&replacement)
                .ok_or_else(|| DocumentError::Html("No editable text in this fragment".into()))?,
        )
    };
    replace_node_with_blocks(&mut state.blocks, node_id, &replacement)?;
    state.selection = Selection::Text(selection.clone());
    state.structure_changed = true;
    Ok(selection)
}

/// Replace only the sequence owning a converted fragment. Immutable sibling
/// allocations and all enclosing table/list identities remain canonical.
fn replace_node_with_blocks(
    blocks: &mut BlockSequence,
    node_id: NodeId,
    replacement: &BlockSequence,
) -> Result<(), DocumentError> {
    let index = blocks
        .top_index_containing(node_id)
        .ok_or(PositionError::UnknownNode(node_id))?;
    let parent_id = blocks.get(index).expect("indexed owner").id();
    if parent_id == node_id {
        let mut next = blocks.to_vec();
        next.splice(index..=index, replacement.iter().cloned());
        *blocks = BlockSequence::new(next);
        return Ok(());
    }
    mutate_node(blocks, parent_id, &mut |parent| {
        match parent {
            BlockNode::List(list) => {
                let mut items = list.items.to_vec();
                let item = items
                    .iter_mut()
                    .find(|item| item.blocks.contains_node(node_id))
                    .ok_or(PositionError::UnknownNode(node_id))?;
                replace_node_with_blocks(&mut item.blocks, node_id, replacement)?;
                list.items = items.into();
            }
            BlockNode::BlockQuote { blocks, .. }
            | BlockNode::Alert { blocks, .. }
            | BlockNode::FootnoteDefinition { blocks, .. } => {
                replace_node_with_blocks(blocks, node_id, replacement)?;
            }
            BlockNode::Table(table) => {
                let mut rows = table.rows.to_vec();
                let row = rows
                    .iter_mut()
                    .find(|row| {
                        row.cells
                            .iter()
                            .any(|cell| cell.blocks.contains_node(node_id))
                    })
                    .ok_or(PositionError::UnknownNode(node_id))?;
                let mut cells = row.cells.to_vec();
                let cell = cells
                    .iter_mut()
                    .find(|cell| cell.blocks.contains_node(node_id))
                    .ok_or(PositionError::UnknownNode(node_id))?;
                replace_node_with_blocks(&mut cell.blocks, node_id, replacement)?;
                row.cells = cells.into();
                table.rows = rows.into();
            }
            _ => return Err(PositionError::UnknownNode(node_id).into()),
        }
        Ok(())
    })
}

fn apply_command(state: &mut SnapshotState, command: EditCommand) -> Result<bool, DocumentError> {
    match command {
        EditCommand::SetSelection(_) | EditCommand::EditPreviewSelection { .. } => {
            unreachable!("handled before transaction")
        }
        EditCommand::ReplaceText {
            node_id,
            range,
            text,
            selection_after,
            ..
        } => {
            mutate_node(&mut state.blocks, node_id, &mut |node| {
                let editable = node.text_mut().ok_or(PositionError::NotText(node_id))?;
                editable.replace(node_id, range.clone(), &text)
            })?;
            state.dirty_nodes.insert(node_id);
            state.selection =
                selection_after.unwrap_or_else(|| caret_after(node_id, range.start + text.len()));
            Ok(true)
        }
        EditCommand::ReplaceSelection { text, typing } => {
            let Selection::Text(selection) = &state.selection else {
                return Err(DocumentError::TableSelectionForTextCommand);
            };
            replace_text_selection(state, selection.clone(), text, typing)
        }
        EditCommand::PasteMarkdown { markdown } => paste_markdown(state, &markdown),
        EditCommand::SplitSelection => split_selection(state),
        EditCommand::ToggleInline {
            node_id,
            range,
            format,
        } => {
            let style = format.as_style();
            let current = find_node(&state.blocks, node_id)
                .ok_or(PositionError::UnknownNode(node_id))?
                .text()
                .ok_or(PositionError::NotText(node_id))?;
            let (covered, total) = current.style_coverage(&range, &style);
            mutate_node(&mut state.blocks, node_id, &mut |node| {
                node.text_mut()
                    .ok_or(PositionError::NotText(node_id))?
                    .set_style(node_id, range.clone(), style.clone(), covered != total)
            })?;
            state.dirty_nodes.insert(node_id);
            Ok(true)
        }
        EditCommand::ToggleInlineSelection { format } => {
            let Selection::Text(selection) = state.selection.clone() else {
                return Err(DocumentError::TableSelectionForTextCommand);
            };
            toggle_inline_selection(state, &selection, &format)
        }
        EditCommand::SetLinkSelection { target } => {
            let Selection::Text(selection) = state.selection.clone() else {
                return Err(DocumentError::TableSelectionForTextCommand);
            };
            set_link_selection(state, &selection, target.as_deref())
        }
        EditCommand::SetBlockStyle { node_id, style } => {
            let wrapper_id =
                matches!(style, BlockStyle::BlockQuote).then(|| allocate_node_id(state));
            mutate_node(&mut state.blocks, node_id, &mut |node| {
                let replacement = match (&*node, style) {
                    (BlockNode::Paragraph(paragraph), BlockStyle::Heading(level)) => {
                        BlockNode::Heading(crate::Heading {
                            id: paragraph.id,
                            level: level.clamp(1, 6),
                            content: paragraph.content.clone(),
                        })
                    }
                    (BlockNode::Heading(heading), BlockStyle::Paragraph) => {
                        BlockNode::Paragraph(crate::Paragraph {
                            id: heading.id,
                            content: heading.content.clone(),
                        })
                    }
                    (BlockNode::CodeBlock(code), BlockStyle::Paragraph) => {
                        BlockNode::Paragraph(crate::Paragraph {
                            id: code.id,
                            content: code.content.clone(),
                        })
                    }
                    (BlockNode::CodeBlock(code), BlockStyle::Heading(level)) => {
                        BlockNode::Heading(crate::Heading {
                            id: code.id,
                            level: level.clamp(1, 6),
                            content: code.content.clone(),
                        })
                    }
                    (BlockNode::Paragraph(paragraph), BlockStyle::CodeBlock) => {
                        BlockNode::CodeBlock(crate::CodeBlock {
                            id: paragraph.id,
                            language: None,
                            syntax: crate::CodeBlockSyntax::Fenced,
                            content: paragraph.content.clone(),
                        })
                    }
                    (BlockNode::Heading(heading), BlockStyle::CodeBlock) => {
                        BlockNode::CodeBlock(crate::CodeBlock {
                            id: heading.id,
                            language: None,
                            syntax: crate::CodeBlockSyntax::Fenced,
                            content: heading.content.clone(),
                        })
                    }
                    (BlockNode::CodeBlock(code), BlockStyle::CodeBlock) => {
                        BlockNode::CodeBlock(code.clone())
                    }
                    (BlockNode::Paragraph(paragraph), BlockStyle::BlockQuote) => {
                        BlockNode::BlockQuote {
                            id: wrapper_id.expect("quote styles allocate a wrapper"),
                            blocks: BlockSequence::new(vec![Arc::new(BlockNode::Paragraph(
                                paragraph.clone(),
                            ))]),
                        }
                    }
                    (BlockNode::Heading(heading), BlockStyle::BlockQuote) => {
                        BlockNode::BlockQuote {
                            id: wrapper_id.expect("quote styles allocate a wrapper"),
                            blocks: BlockSequence::new(vec![Arc::new(BlockNode::Heading(
                                heading.clone(),
                            ))]),
                        }
                    }
                    (_, BlockStyle::Paragraph | BlockStyle::Heading(_)) => {
                        return Err(PositionError::NotText(node_id).into());
                    }
                    (_, BlockStyle::BlockQuote | BlockStyle::CodeBlock) => {
                        return Err(PositionError::NotText(node_id).into());
                    }
                };
                *node = replacement;
                Ok(())
            })?;
            state.dirty_nodes.insert(node_id);
            state.structure_changed |= wrapper_id.is_some();
            Ok(true)
        }
        EditCommand::SetImageAttributes {
            image_id,
            source,
            alt,
        } => {
            let mut changed = false;
            mutate_node(&mut state.blocks, image_id, &mut |node| {
                let BlockNode::Image(image) = node else {
                    return Err(DocumentError::NotImage(image_id));
                };
                if image.source != source || image.alt.as_string() != alt {
                    image.source.clone_from(&source);
                    image.alt = crate::RichText::new(&alt);
                    image.intrinsic_size = None;
                    changed = true;
                }
                Ok(())
            })?;
            if changed {
                state.dirty_nodes.insert(image_id);
            }
            Ok(changed)
        }
        EditCommand::InsertBlock { index, block } => {
            let mut blocks = state.blocks.to_vec();
            blocks.insert(index.min(blocks.len()), block);
            state.blocks = BlockSequence::new(blocks);
            state.structure_changed = true;
            Ok(true)
        }
        EditCommand::InsertBlockAfterSelection { kind } => {
            let selected_node = match &state.selection {
                Selection::Text(selection) => selection.head.node_id,
                Selection::Table(selection) => selection.table_id,
            };
            let index = state
                .blocks
                .iter()
                .position(|block| {
                    block.id() == selected_node || block_contains_node(block, selected_node)
                })
                .map_or(state.blocks.len(), |index| index + 1);
            let block = new_block(state, kind);
            let position = first_editable_in_block(&block);
            let mut blocks = state.blocks.to_vec();
            blocks.insert(index, Arc::new(block));
            state.blocks = BlockSequence::new(blocks);
            if let Some(position) = position {
                state.selection = Selection::Text(TextSelection::caret(position));
            }
            state.structure_changed = true;
            Ok(true)
        }
        EditCommand::DeleteBlock { node_id } => {
            let mut blocks = state.blocks.to_vec();
            let Some(index) = blocks.iter().position(|block| block.id() == node_id) else {
                return Err(PositionError::UnknownNode(node_id).into());
            };
            blocks.remove(index);
            state.blocks = BlockSequence::new(blocks);
            state.structure_changed = true;
            Ok(true)
        }
        EditCommand::ConvertHtmlToMarkdown { node_id }
        | EditCommand::ConvertHtmlToMarkdownAt { node_id, .. }
        | EditCommand::EditHtmlSelection { node_id, .. } => {
            let target = match &command {
                EditCommand::ConvertHtmlToMarkdownAt {
                    expected_source,
                    position,
                    ..
                } => Some((expected_source.as_str(), *position, *position)),
                EditCommand::EditHtmlSelection {
                    expected_source,
                    anchor,
                    head,
                    ..
                } => Some((expected_source.as_str(), *anchor, *head)),
                _ => None,
            };
            let selection = convert_html_to_markdown(state, node_id, target)?;
            if let EditCommand::EditHtmlSelection {
                edit: crate::HtmlTextEdit::Replace(text),
                ..
            } = &command
                && text.is_empty()
                && selection.anchor.node_id == selection.head.node_id
                && selection.anchor.text_offset == selection.head.text_offset
            {
                return Ok(false);
            }
            if let EditCommand::EditHtmlSelection { edit, .. } = command {
                apply_command(state, edit.into_command())?;
            }
            Ok(true)
        }
        EditCommand::InsertTable { index } => {
            let mut next = || allocate_node_id(state);
            let table = Arc::new(BlockNode::Table(Table::new_default(&mut next)));
            let mut blocks = state.blocks.to_vec();
            blocks.insert(index.min(blocks.len()), table);
            state.blocks = BlockSequence::new(blocks);
            state.structure_changed = true;
            Ok(true)
        }
        EditCommand::InsertTableRow { table_id, index } => {
            mutate_table_with_ids(state, table_id, |table, allocate| {
                table.insert_row(index, allocate)
            })
        }
        EditCommand::DeleteTableRow { table_id, index } => {
            mutate_table(state, table_id, |table| table.delete_row(index))
        }
        EditCommand::MoveTableRow { table_id, from, to } => {
            mutate_table(state, table_id, |table| table.move_row(from, to))
        }
        EditCommand::InsertTableColumn { table_id, index } => {
            mutate_table_with_ids(state, table_id, |table, allocate| {
                table.insert_column(index, allocate)
            })
        }
        EditCommand::DeleteTableColumn { table_id, index } => {
            mutate_table(state, table_id, |table| table.delete_column(index))
        }
        EditCommand::MoveTableColumn { table_id, from, to } => {
            mutate_table(state, table_id, |table| table.move_column(from, to))
        }
        EditCommand::SetTableColumnAlignment {
            table_id,
            column,
            alignment,
        } => mutate_table(state, table_id, |table| {
            table.set_alignment(column, alignment)
        }),
        EditCommand::SetTableColumnWidth {
            table_id,
            column,
            width,
        } => mutate_table(state, table_id, |table| table.set_width(column, width)),
        EditCommand::SetTableBorder { table_id, border } => {
            mutate_table(state, table_id, |table| {
                table.border = border;
                Ok(())
            })
        }
        EditCommand::PasteTsv {
            table_id,
            row,
            column,
            text,
        } => mutate_table_with_ids(state, table_id, |table, allocate| {
            table.paste_tsv(row, column, &text, allocate)
        }),
        EditCommand::ToggleTask { item_id } => {
            mutate_list_item(&mut state.blocks, item_id)?;
            state.dirty_nodes.insert(item_id);
            Ok(true)
        }
        EditCommand::IndentListItem { item_id } => {
            let mut next_id = state.next_node_id;
            let changed = indent_list_item(&mut state.blocks, item_id, &mut next_id)?
                .ok_or(PositionError::UnknownNode(item_id))?;
            state.next_node_id = next_id;
            if changed {
                state.structure_changed = true;
                state.dirty_nodes.insert(item_id);
            }
            Ok(changed)
        }
        EditCommand::OutdentListItem { item_id } => {
            let changed = outdent_list_item(&mut state.blocks, item_id, true)?;
            if changed {
                state.structure_changed = true;
                state.dirty_nodes.insert(item_id);
            }
            Ok(changed)
        }
    }
}

fn indent_list_item(
    blocks: &mut BlockSequence,
    item_id: NodeId,
    next_id: &mut u64,
) -> Result<Option<bool>, DocumentError> {
    let mut sequence = blocks.to_vec();
    for block_index in 0..sequence.len() {
        if !block_contains_node(sequence[block_index].as_ref(), item_id) {
            continue;
        }
        if let BlockNode::List(list) = Arc::make_mut(&mut sequence[block_index]) {
            let mut items = list.items.to_vec();
            if let Some(item_index) = items.iter().position(|item| item.id == item_id) {
                if item_index == 0 {
                    return Ok(Some(false));
                }
                let item = items.remove(item_index);
                let parent = &mut items[item_index - 1];
                let mut parent_blocks = parent.blocks.to_vec();
                if let Some(BlockNode::List(nested)) = parent_blocks.last_mut().map(Arc::make_mut)
                    && nested.kind == list.kind
                {
                    let mut nested_items = nested.items.to_vec();
                    nested_items.push(item);
                    nested.items = nested_items.into();
                } else {
                    parent_blocks.push(Arc::new(BlockNode::List(crate::ListBlock {
                        id: allocate_from(next_id),
                        kind: list.kind.clone(),
                        tight: list.tight,
                        items: vec![item].into(),
                    })));
                }
                parent.blocks = BlockSequence::new(parent_blocks);
                list.items = items.into();
                *blocks = BlockSequence::new(sequence);
                return Ok(Some(true));
            }

            for item in &mut items {
                if let Some(changed) = indent_list_item(&mut item.blocks, item_id, next_id)? {
                    list.items = items.into();
                    *blocks = BlockSequence::new(sequence);
                    return Ok(Some(changed));
                }
            }
            list.items = items.into();
        }

        let found = match Arc::make_mut(&mut sequence[block_index]) {
            BlockNode::BlockQuote { blocks, .. }
            | BlockNode::Alert { blocks, .. }
            | BlockNode::FootnoteDefinition { blocks, .. } => {
                indent_list_item(blocks, item_id, next_id)?
            }
            BlockNode::Table(table) => {
                let mut found = None;
                let mut rows = table.rows.to_vec();
                'rows: for row in &mut rows {
                    let mut cells = row.cells.to_vec();
                    for cell in &mut cells {
                        if let Some(changed) = indent_list_item(&mut cell.blocks, item_id, next_id)?
                        {
                            found = Some(changed);
                            row.cells = cells.into();
                            break 'rows;
                        }
                    }
                }
                if found.is_some() {
                    table.rows = rows.into();
                }
                found
            }
            _ => None,
        };
        if found.is_some() {
            *blocks = BlockSequence::new(sequence);
            return Ok(found);
        }
    }
    Ok(None)
}

enum OutdentResult {
    NotFound,
    Handled,
    Bubble(crate::ListItem),
}

fn outdent_list_item(
    blocks: &mut BlockSequence,
    item_id: NodeId,
    escape_direct: bool,
) -> Result<bool, DocumentError> {
    match outdent_in_sequence(blocks, item_id, escape_direct)? {
        OutdentResult::Handled => Ok(true),
        OutdentResult::NotFound => Err(PositionError::UnknownNode(item_id).into()),
        OutdentResult::Bubble(_) => {
            debug_assert!(!escape_direct, "root outdent must consume direct items");
            Ok(false)
        }
    }
}

fn outdent_in_sequence(
    blocks: &mut BlockSequence,
    item_id: NodeId,
    escape_direct: bool,
) -> Result<OutdentResult, DocumentError> {
    let mut sequence = blocks.to_vec();
    for block_index in 0..sequence.len() {
        if !block_contains_node(sequence[block_index].as_ref(), item_id) {
            continue;
        }
        if let BlockNode::List(list) = Arc::make_mut(&mut sequence[block_index]) {
            let mut items = list.items.to_vec();
            if let Some(item_index) = items.iter().position(|item| item.id == item_id) {
                let item = items.remove(item_index);
                list.items = items.into();
                if escape_direct {
                    let insertion = if list.items.is_empty() {
                        sequence.remove(block_index);
                        block_index
                    } else {
                        block_index + 1
                    };
                    sequence.splice(insertion..insertion, item.blocks.to_vec());
                    *blocks = BlockSequence::new(sequence);
                    return Ok(OutdentResult::Handled);
                }
                *blocks = BlockSequence::new(sequence);
                return Ok(OutdentResult::Bubble(item));
            }

            for parent_index in 0..items.len() {
                match outdent_in_sequence(&mut items[parent_index].blocks, item_id, false)? {
                    OutdentResult::Bubble(item) => {
                        items.insert(parent_index + 1, item);
                        list.items = items.into();
                        *blocks = BlockSequence::new(sequence);
                        return Ok(OutdentResult::Handled);
                    }
                    OutdentResult::Handled => {
                        list.items = items.into();
                        *blocks = BlockSequence::new(sequence);
                        return Ok(OutdentResult::Handled);
                    }
                    OutdentResult::NotFound => {}
                }
            }
            list.items = items.into();
        }

        let result = match Arc::make_mut(&mut sequence[block_index]) {
            BlockNode::BlockQuote { blocks, .. }
            | BlockNode::Alert { blocks, .. }
            | BlockNode::FootnoteDefinition { blocks, .. } => {
                outdent_in_sequence(blocks, item_id, true)?
            }
            BlockNode::Table(table) => {
                let mut result = OutdentResult::NotFound;
                let mut rows = table.rows.to_vec();
                'rows: for row in &mut rows {
                    let mut cells = row.cells.to_vec();
                    for cell in &mut cells {
                        let candidate = outdent_in_sequence(&mut cell.blocks, item_id, true)?;
                        if !matches!(candidate, OutdentResult::NotFound) {
                            result = candidate;
                            row.cells = cells.into();
                            break 'rows;
                        }
                    }
                }
                if !matches!(result, OutdentResult::NotFound) {
                    table.rows = rows.into();
                }
                result
            }
            _ => OutdentResult::NotFound,
        };
        if !matches!(result, OutdentResult::NotFound) {
            *blocks = BlockSequence::new(sequence);
            return Ok(result);
        }
    }
    Ok(OutdentResult::NotFound)
}

fn mutate_table(
    state: &mut SnapshotState,
    table_id: NodeId,
    mut operation: impl FnMut(&mut Table) -> Result<(), DocumentError>,
) -> Result<bool, DocumentError> {
    mutate_node(&mut state.blocks, table_id, &mut |node| {
        let BlockNode::Table(table) = node else {
            return Err(DocumentError::NotTable(table_id));
        };
        operation(table)
    })?;
    state.dirty_nodes.insert(table_id);
    Ok(true)
}

fn mutate_table_with_ids(
    state: &mut SnapshotState,
    table_id: NodeId,
    mut operation: impl FnMut(&mut Table, &mut dyn FnMut() -> NodeId) -> Result<(), DocumentError>,
) -> Result<bool, DocumentError> {
    let mut next_id = state.next_node_id;
    mutate_node(&mut state.blocks, table_id, &mut |node| {
        let BlockNode::Table(table) = node else {
            return Err(DocumentError::NotTable(table_id));
        };
        let mut allocate = || {
            let id = NodeId::new_unchecked(next_id);
            next_id += 1;
            id
        };
        operation(table, &mut allocate)
    })?;
    state.next_node_id = next_id;
    state.dirty_nodes.insert(table_id);
    Ok(true)
}

fn allocate_node_id(state: &mut SnapshotState) -> NodeId {
    let id = NodeId::new_unchecked(state.next_node_id);
    state.next_node_id += 1;
    id
}

fn new_block(state: &mut SnapshotState, kind: InsertBlockKind) -> BlockNode {
    match kind {
        InsertBlockKind::Paragraph => BlockNode::Paragraph(crate::Paragraph {
            id: allocate_node_id(state),
            content: crate::RichText::default(),
        }),
        InsertBlockKind::Heading(level) => BlockNode::Heading(crate::Heading {
            id: allocate_node_id(state),
            level: level.clamp(1, 6),
            content: crate::RichText::default(),
        }),
        InsertBlockKind::OrderedList
        | InsertBlockKind::UnorderedList
        | InsertBlockKind::TaskList => {
            let list_id = allocate_node_id(state);
            let item_id = allocate_node_id(state);
            let paragraph_id = allocate_node_id(state);
            BlockNode::List(crate::ListBlock {
                id: list_id,
                kind: match kind {
                    InsertBlockKind::OrderedList => crate::ListKind::Ordered { start: 1 },
                    InsertBlockKind::TaskList => crate::ListKind::Task,
                    _ => crate::ListKind::Unordered,
                },
                tight: false,
                items: vec![crate::ListItem {
                    id: item_id,
                    checked: matches!(kind, InsertBlockKind::TaskList).then_some(false),
                    blocks: BlockSequence::new(vec![Arc::new(BlockNode::Paragraph(
                        crate::Paragraph {
                            id: paragraph_id,
                            content: crate::RichText::default(),
                        },
                    ))]),
                }]
                .into(),
            })
        }
        InsertBlockKind::BlockQuote => {
            let quote_id = allocate_node_id(state);
            let paragraph_id = allocate_node_id(state);
            BlockNode::BlockQuote {
                id: quote_id,
                blocks: BlockSequence::new(vec![Arc::new(BlockNode::Paragraph(
                    crate::Paragraph {
                        id: paragraph_id,
                        content: crate::RichText::default(),
                    },
                ))]),
            }
        }
        InsertBlockKind::CodeBlock => BlockNode::CodeBlock(crate::CodeBlock {
            id: allocate_node_id(state),
            language: None,
            syntax: crate::CodeBlockSyntax::Fenced,
            content: crate::RichText::default(),
        }),
        InsertBlockKind::Image => BlockNode::Image(crate::ImageNode {
            id: allocate_node_id(state),
            source: String::new(),
            alt: crate::RichText::default(),
            title: None,
            intrinsic_size: None,
            link: None,
        }),
        InsertBlockKind::Table => {
            let mut next = || allocate_node_id(state);
            BlockNode::Table(Table::new_default(&mut next))
        }
        InsertBlockKind::ThematicBreak => BlockNode::ThematicBreak {
            id: allocate_node_id(state),
        },
    }
}

fn first_editable_in_block(block: &BlockNode) -> Option<DocumentPosition> {
    if block.text().is_some() {
        return Some(DocumentPosition::new(
            block.id(),
            0,
            crate::Affinity::Downstream,
        ));
    }
    match block {
        BlockNode::List(list) => list.items.iter().find_map(|item| {
            item.blocks
                .iter()
                .find_map(|block| first_editable_in_block(block))
        }),
        BlockNode::BlockQuote { blocks, .. }
        | BlockNode::Alert { blocks, .. }
        | BlockNode::FootnoteDefinition { blocks, .. } => blocks
            .iter()
            .find_map(|block| first_editable_in_block(block)),
        BlockNode::Table(table) => table.rows.iter().find_map(|row| {
            row.cells.iter().find_map(|cell| {
                cell.blocks
                    .iter()
                    .find_map(|block| first_editable_in_block(block))
            })
        }),
        _ => None,
    }
}

fn block_contains_node(block: &BlockNode, node_id: NodeId) -> bool {
    match block {
        BlockNode::List(list) => list.items.iter().any(|item| {
            item.id == node_id
                || item
                    .blocks
                    .iter()
                    .any(|block| block.id() == node_id || block_contains_node(block, node_id))
        }),
        BlockNode::BlockQuote { blocks, .. }
        | BlockNode::Alert { blocks, .. }
        | BlockNode::FootnoteDefinition { blocks, .. } => blocks
            .iter()
            .any(|block| block.id() == node_id || block_contains_node(block, node_id)),
        BlockNode::Table(table) => table.rows.iter().any(|row| {
            row.id == node_id
                || row.cells.iter().any(|cell| {
                    cell.id == node_id
                        || cell.blocks.iter().any(|block| {
                            block.id() == node_id || block_contains_node(block, node_id)
                        })
                })
        }),
        _ => false,
    }
}

fn normalized_single_node_range(
    selection: &TextSelection,
) -> Result<std::ops::Range<usize>, DocumentError> {
    if selection.anchor.node_id != selection.head.node_id {
        return Err(DocumentError::Markdown(
            "cross-block replacement is not yet representable by this command".into(),
        ));
    }
    Ok(selection.anchor.text_offset.min(selection.head.text_offset)
        ..selection.anchor.text_offset.max(selection.head.text_offset))
}

fn validate_composition_range(
    state: &SnapshotState,
    range: &TextSelection,
) -> Result<(), DocumentError> {
    let mut probe = state.clone();
    replace_text_selection(&mut probe, range.clone(), String::new(), false)?;
    validate_tree(&probe.blocks)
}

fn replace_text_selection(
    state: &mut SnapshotState,
    selection: TextSelection,
    text: String,
    typing: bool,
) -> Result<bool, DocumentError> {
    if selection.anchor.node_id == selection.head.node_id {
        let range = normalized_single_node_range(&selection)?;
        return apply_command(
            state,
            EditCommand::ReplaceText {
                node_id: selection.anchor.node_id,
                range,
                text,
                selection_after: None,
                typing,
            },
        );
    }

    let anchor_index = state
        .blocks
        .iter()
        .position(|block| block.id() == selection.anchor.node_id);
    let head_index = state
        .blocks
        .iter()
        .position(|block| block.id() == selection.head.node_id);
    let (Some(anchor_index), Some(head_index)) = (anchor_index, head_index) else {
        return Err(DocumentError::Markdown(
            "cross-block replacement must begin and end in top-level editable blocks".into(),
        ));
    };
    let (start_index, start_position, end_index, end_position) = if anchor_index < head_index {
        (anchor_index, selection.anchor, head_index, selection.head)
    } else {
        (head_index, selection.head, anchor_index, selection.anchor)
    };
    let suffix = state
        .blocks
        .get(end_index)
        .expect("the index was resolved from this sequence")
        .text()
        .ok_or(PositionError::NotText(end_position.node_id))?
        .clone();
    suffix.validate_range(
        end_position.node_id,
        &(end_position.text_offset..end_position.text_offset),
    )?;

    let mut blocks = state.blocks.to_vec();
    let first = Arc::make_mut(&mut blocks[start_index]);
    first
        .text_mut()
        .ok_or(PositionError::NotText(start_position.node_id))?
        .replace_tail_with_text_and_suffix(
            start_position.node_id,
            start_position.text_offset,
            &text,
            &suffix,
            end_position.text_offset,
        )?;
    blocks.drain(start_index + 1..=end_index);
    state.blocks = BlockSequence::new(blocks);
    state.structure_changed = true;
    state.dirty_nodes.insert(start_position.node_id);
    state.selection = caret_after(
        start_position.node_id,
        start_position.text_offset + text.len(),
    );
    Ok(true)
}

fn replace_rich_text_selection(
    state: &mut SnapshotState,
    selection: TextSelection,
    replacement: &crate::RichText,
) -> Result<bool, DocumentError> {
    if selection.anchor.node_id == selection.head.node_id {
        let range = normalized_single_node_range(&selection)?;
        let node_id = selection.anchor.node_id;
        mutate_node(&mut state.blocks, node_id, &mut |node| {
            node.text_mut()
                .ok_or(PositionError::NotText(node_id))?
                .replace_rich(node_id, range.clone(), replacement)
        })?;
        state.dirty_nodes.insert(node_id);
        state.selection = caret_after(node_id, range.start + replacement.len());
        return Ok(!range.is_empty() || !replacement.is_empty());
    }

    let anchor_index = state
        .blocks
        .iter()
        .position(|block| block.id() == selection.anchor.node_id);
    let head_index = state
        .blocks
        .iter()
        .position(|block| block.id() == selection.head.node_id);
    let (Some(anchor_index), Some(head_index)) = (anchor_index, head_index) else {
        return Err(DocumentError::Clipboard(
            "rich cross-block paste requires top-level editable blocks".into(),
        ));
    };
    let (start_index, start_position, end_index, end_position) = if anchor_index < head_index {
        (anchor_index, selection.anchor, head_index, selection.head)
    } else {
        (head_index, selection.head, anchor_index, selection.anchor)
    };
    let suffix = state
        .blocks
        .get(end_index)
        .expect("the index was resolved from this sequence")
        .text()
        .ok_or(PositionError::NotText(end_position.node_id))?
        .clone();
    let mut blocks = state.blocks.to_vec();
    let first = Arc::make_mut(&mut blocks[start_index]);
    first
        .text_mut()
        .ok_or(PositionError::NotText(start_position.node_id))?
        .replace_tail_with_rich_and_suffix(
            start_position.node_id,
            start_position.text_offset,
            replacement,
            &suffix,
            end_position.text_offset,
        )?;
    blocks.drain(start_index + 1..=end_index);
    state.blocks = BlockSequence::new(blocks);
    state.structure_changed = true;
    state.dirty_nodes.insert(start_position.node_id);
    state.selection = caret_after(
        start_position.node_id,
        start_position.text_offset + replacement.len(),
    );
    Ok(true)
}

fn paste_markdown(state: &mut SnapshotState, markdown: &str) -> Result<bool, DocumentError> {
    let imported = crate::markdown::import(Arc::<str>::from(markdown))?;
    if imported.blocks().len() == 1
        && let Some(BlockNode::Paragraph(paragraph)) = imported.blocks().get(0).map(AsRef::as_ref)
    {
        let Selection::Text(selection) = state.selection.clone() else {
            return Err(DocumentError::TableSelectionForTextCommand);
        };
        return replace_rich_text_selection(state, selection, &paragraph.content);
    }

    let Selection::Text(selection) = state.selection.clone() else {
        return Err(DocumentError::TableSelectionForTextCommand);
    };
    let anchor_index = state
        .blocks
        .iter()
        .position(|block| block.id() == selection.anchor.node_id);
    let head_index = state
        .blocks
        .iter()
        .position(|block| block.id() == selection.head.node_id);
    let (Some(anchor_index), Some(head_index)) = (anchor_index, head_index) else {
        return Err(DocumentError::Clipboard(
            "block Markdown paste requires a top-level text selection".into(),
        ));
    };
    let (start_index, start_position, end_index, end_position) = if anchor_index < head_index
        || (anchor_index == head_index
            && selection.anchor.text_offset <= selection.head.text_offset)
    {
        (anchor_index, selection.anchor, head_index, selection.head)
    } else {
        (head_index, selection.head, anchor_index, selection.anchor)
    };

    let start_block = state
        .blocks
        .get(start_index)
        .expect("the index was resolved from this sequence")
        .clone();
    let end_block = state
        .blocks
        .get(end_index)
        .expect("the index was resolved from this sequence")
        .clone();
    let start_text = start_block
        .text()
        .ok_or(PositionError::NotText(start_position.node_id))?;
    let end_text = end_block
        .text()
        .ok_or(PositionError::NotText(end_position.node_id))?;
    start_text.validate_range(
        start_position.node_id,
        &(start_position.text_offset..start_position.text_offset),
    )?;
    end_text.validate_range(
        end_position.node_id,
        &(end_position.text_offset..end_position.text_offset),
    )?;
    let prefix = start_text.slice(0..start_position.text_offset);
    let suffix = end_text.slice(end_position.text_offset..end_text.len());
    let has_prefix = !prefix.is_empty();

    let mut inserted = imported
        .blocks()
        .iter()
        .map(|block| remap_block_ids(block, &mut state.next_node_id))
        .collect::<Vec<_>>();
    let caret = inserted
        .iter()
        .rev()
        .find_map(|block| last_editable_in_block(block));
    let mut replacement = Vec::with_capacity(inserted.len() + 2);
    if has_prefix {
        replacement.push(Arc::new(block_with_text(
            &start_block,
            start_block.id(),
            prefix,
        )?));
    }
    replacement.append(&mut inserted);
    if !suffix.is_empty() {
        let suffix_id = if start_index == end_index && has_prefix {
            allocate_from(&mut state.next_node_id)
        } else if start_index == end_index {
            start_block.id()
        } else {
            end_block.id()
        };
        replacement.push(Arc::new(block_with_text(&end_block, suffix_id, suffix)?));
    }
    if replacement.is_empty() {
        replacement.push(Arc::new(BlockNode::Paragraph(crate::Paragraph {
            id: start_block.id(),
            content: crate::RichText::default(),
        })));
    }

    let selection_after = caret.or_else(|| {
        replacement
            .iter()
            .rev()
            .find_map(|block| last_editable_in_block(block))
    });
    let mut blocks = state.blocks.to_vec();
    blocks.splice(start_index..=end_index, replacement);
    state.blocks = BlockSequence::new(blocks);
    state.structure_changed = true;
    if let Some(position) = selection_after {
        state.selection = Selection::Text(TextSelection::caret(position));
    }
    Ok(true)
}

fn block_with_text(
    source: &BlockNode,
    id: NodeId,
    content: crate::RichText,
) -> Result<BlockNode, DocumentError> {
    match source {
        BlockNode::Paragraph(_) => Ok(BlockNode::Paragraph(crate::Paragraph { id, content })),
        BlockNode::Heading(heading) => Ok(BlockNode::Heading(crate::Heading {
            id,
            level: heading.level,
            content,
        })),
        BlockNode::CodeBlock(code) => Ok(BlockNode::CodeBlock(crate::CodeBlock {
            id,
            language: code.language.clone(),
            syntax: code.syntax,
            content,
        })),
        _ => Err(DocumentError::Clipboard(
            "block Markdown paste requires paragraph, heading, or code boundaries".into(),
        )),
    }
}

fn remap_sequence_ids(blocks: &BlockSequence, next_id: &mut u64) -> BlockSequence {
    BlockSequence::new(
        blocks
            .iter()
            .map(|block| remap_block_ids(block, next_id))
            .collect(),
    )
}

fn remap_block_ids(block: &BlockNode, next_id: &mut u64) -> Arc<BlockNode> {
    let block = match block {
        BlockNode::Paragraph(paragraph) => BlockNode::Paragraph(crate::Paragraph {
            id: allocate_from(next_id),
            content: paragraph.content.clone(),
        }),
        BlockNode::Heading(heading) => BlockNode::Heading(crate::Heading {
            id: allocate_from(next_id),
            level: heading.level,
            content: heading.content.clone(),
        }),
        BlockNode::CodeBlock(code) => BlockNode::CodeBlock(crate::CodeBlock {
            id: allocate_from(next_id),
            language: code.language.clone(),
            syntax: code.syntax,
            content: code.content.clone(),
        }),
        BlockNode::Image(image) => BlockNode::Image(crate::ImageNode {
            id: allocate_from(next_id),
            source: image.source.clone(),
            alt: image.alt.clone(),
            title: image.title.clone(),
            intrinsic_size: image.intrinsic_size,
            link: image.link.clone(),
        }),
        BlockNode::List(list) => BlockNode::List(crate::ListBlock {
            id: allocate_from(next_id),
            kind: list.kind.clone(),
            tight: list.tight,
            items: list
                .items
                .iter()
                .map(|item| crate::ListItem {
                    id: allocate_from(next_id),
                    checked: item.checked,
                    blocks: remap_sequence_ids(&item.blocks, next_id),
                })
                .collect::<Vec<_>>()
                .into(),
        }),
        BlockNode::BlockQuote { blocks, .. } => BlockNode::BlockQuote {
            id: allocate_from(next_id),
            blocks: remap_sequence_ids(blocks, next_id),
        },
        BlockNode::Table(table) => BlockNode::Table(crate::Table {
            id: allocate_from(next_id),
            columns: table.columns.clone(),
            rows: table
                .rows
                .iter()
                .map(|row| crate::TableRow {
                    id: allocate_from(next_id),
                    cells: row
                        .cells
                        .iter()
                        .map(|cell| crate::TableCell {
                            id: allocate_from(next_id),
                            blocks: remap_sequence_ids(&cell.blocks, next_id),
                        })
                        .collect::<Vec<_>>()
                        .into(),
                })
                .collect::<Vec<_>>()
                .into(),
            header_rows: table.header_rows,
            border: table.border,
            preserved_metadata: table.preserved_metadata.clone(),
        }),
        BlockNode::Alert {
            kind,
            title,
            blocks,
            ..
        } => BlockNode::Alert {
            id: allocate_from(next_id),
            kind: kind.clone(),
            title: title.clone(),
            blocks: remap_sequence_ids(blocks, next_id),
        },
        BlockNode::FootnoteDefinition { label, blocks, .. } => BlockNode::FootnoteDefinition {
            id: allocate_from(next_id),
            label: label.clone(),
            blocks: remap_sequence_ids(blocks, next_id),
        },
        BlockNode::ThematicBreak { .. } => BlockNode::ThematicBreak {
            id: allocate_from(next_id),
        },
        BlockNode::PreservedSource {
            source,
            description,
            ..
        } => BlockNode::PreservedSource {
            id: allocate_from(next_id),
            source: source.clone(),
            description: description.clone(),
        },
    };
    Arc::new(block)
}

fn last_editable_in_block(block: &BlockNode) -> Option<DocumentPosition> {
    if let Some(text) = block.text() {
        return Some(DocumentPosition::new(
            block.id(),
            text.len(),
            crate::Affinity::Downstream,
        ));
    }
    match block {
        BlockNode::List(list) => list.items.iter().rev().find_map(|item| {
            item.blocks
                .iter()
                .rev()
                .find_map(|block| last_editable_in_block(block))
        }),
        BlockNode::BlockQuote { blocks, .. }
        | BlockNode::Alert { blocks, .. }
        | BlockNode::FootnoteDefinition { blocks, .. } => blocks
            .iter()
            .rev()
            .find_map(|block| last_editable_in_block(block)),
        BlockNode::Table(table) => table.rows.iter().rev().find_map(|row| {
            row.cells.iter().rev().find_map(|cell| {
                cell.blocks
                    .iter()
                    .rev()
                    .find_map(|block| last_editable_in_block(block))
            })
        }),
        _ => None,
    }
}

fn split_selection(state: &mut SnapshotState) -> Result<bool, DocumentError> {
    let Selection::Text(mut selection) = state.selection.clone() else {
        return Err(DocumentError::TableSelectionForTextCommand);
    };
    if !selection.is_caret() {
        replace_text_selection(state, selection, String::new(), false)?;
        let Selection::Text(current) = state.selection.clone() else {
            unreachable!("text replacement leaves a text selection");
        };
        selection = current;
    }
    let node_id = selection.head.node_id;
    let offset = selection.head.text_offset;
    if matches!(
        find_node(&state.blocks, node_id),
        Some(BlockNode::CodeBlock(_))
    ) {
        return apply_command(
            state,
            EditCommand::ReplaceText {
                node_id,
                range: offset..offset,
                text: "\n".into(),
                selection_after: None,
                typing: false,
            },
        );
    }
    let mut next_id = state.next_node_id;
    let Some(position) = split_in_sequence(&mut state.blocks, node_id, offset, &mut next_id)?
    else {
        return Err(PositionError::NotText(node_id).into());
    };
    state.next_node_id = next_id;
    state.selection = Selection::Text(TextSelection::caret(position));
    state.structure_changed = true;
    state.dirty_nodes.insert(node_id);
    Ok(true)
}

struct ListSplit {
    position: DocumentPosition,
    escaped_blocks: Option<Vec<Arc<BlockNode>>>,
}

fn split_in_sequence(
    blocks: &mut BlockSequence,
    node_id: NodeId,
    offset: usize,
    next_id: &mut u64,
) -> Result<Option<DocumentPosition>, DocumentError> {
    let mut next = blocks.to_vec();
    for index in 0..next.len() {
        if next[index].id() == node_id {
            let new_id = allocate_from(next_id);
            let (left, right) = split_block(next[index].as_ref(), new_id, offset)?;
            next[index] = Arc::new(left);
            next.insert(index + 1, Arc::new(right));
            *blocks = BlockSequence::new(next);
            return Ok(Some(DocumentPosition::new(
                new_id,
                0,
                crate::Affinity::Downstream,
            )));
        }

        if !block_contains_node(next[index].as_ref(), node_id) {
            continue;
        }

        let list_split = if let BlockNode::List(list) = Arc::make_mut(&mut next[index]) {
            split_in_list(list, node_id, offset, next_id)?
        } else {
            None
        };
        if let Some(list_split) = list_split {
            if let Some(escaped) = list_split.escaped_blocks {
                let list_empty =
                    matches!(next[index].as_ref(), BlockNode::List(list) if list.items.is_empty());
                let insertion = if list_empty {
                    next.remove(index);
                    index
                } else {
                    index + 1
                };
                next.splice(insertion..insertion, escaped);
            }
            *blocks = BlockSequence::new(next);
            return Ok(Some(list_split.position));
        }

        let found = match Arc::make_mut(&mut next[index]) {
            BlockNode::BlockQuote { blocks, .. }
            | BlockNode::Alert { blocks, .. }
            | BlockNode::FootnoteDefinition { blocks, .. } => {
                split_in_sequence(blocks, node_id, offset, next_id)?
            }
            BlockNode::Table(table) => {
                let mut found = None;
                let mut rows = table.rows.to_vec();
                'rows: for row in &mut rows {
                    let mut cells = row.cells.to_vec();
                    for cell in &mut cells {
                        if let Some(position) =
                            split_in_sequence(&mut cell.blocks, node_id, offset, next_id)?
                        {
                            found = Some(position);
                            row.cells = cells.into();
                            break 'rows;
                        }
                    }
                }
                if found.is_some() {
                    table.rows = rows.into();
                }
                found
            }
            _ => None,
        };
        if found.is_some() {
            *blocks = BlockSequence::new(next);
            return Ok(found);
        }
    }
    Ok(None)
}

fn split_in_list(
    list: &mut crate::ListBlock,
    node_id: NodeId,
    offset: usize,
    next_id: &mut u64,
) -> Result<Option<ListSplit>, DocumentError> {
    let mut items = list.items.to_vec();
    for item_index in 0..items.len() {
        let block_index = {
            items[item_index]
                .blocks
                .iter()
                .position(|block| block.id() == node_id)
        };
        if let Some(block_index) = block_index {
            let target = items[item_index]
                .blocks
                .get(block_index)
                .expect("block index resolved above");
            let target_text = target.text().ok_or(PositionError::NotText(node_id))?;
            target_text.validate_range(node_id, &(offset..offset))?;
            if target_text.is_empty() && offset == 0 {
                let escaped = items.remove(item_index).blocks.to_vec();
                list.items = items.into();
                return Ok(Some(ListSplit {
                    position: DocumentPosition::new(node_id, 0, crate::Affinity::Downstream),
                    escaped_blocks: Some(escaped),
                }));
            }

            let right_id = allocate_from(next_id);
            let item_id = allocate_from(next_id);
            let (left, right) = split_block(target, right_id, offset)?;
            let original_blocks = items[item_index].blocks.to_vec();
            let mut first_blocks = original_blocks[..block_index].to_vec();
            first_blocks.push(Arc::new(left));
            let mut second_blocks = vec![Arc::new(right)];
            second_blocks.extend_from_slice(&original_blocks[block_index + 1..]);
            items[item_index].blocks = BlockSequence::new(first_blocks);
            items.insert(
                item_index + 1,
                crate::ListItem {
                    id: item_id,
                    checked: matches!(list.kind, crate::ListKind::Task).then_some(false),
                    blocks: BlockSequence::new(second_blocks),
                },
            );
            list.items = items.into();
            return Ok(Some(ListSplit {
                position: DocumentPosition::new(right_id, 0, crate::Affinity::Downstream),
                escaped_blocks: None,
            }));
        }

        if let Some(position) =
            split_in_sequence(&mut items[item_index].blocks, node_id, offset, next_id)?
        {
            list.items = items.into();
            return Ok(Some(ListSplit {
                position,
                escaped_blocks: None,
            }));
        }
    }
    Ok(None)
}

fn split_block(
    block: &BlockNode,
    right_id: NodeId,
    offset: usize,
) -> Result<(BlockNode, BlockNode), DocumentError> {
    Ok(match block {
        BlockNode::Paragraph(paragraph) => {
            let (left, right) = paragraph.content.split_at(paragraph.id, offset)?;
            (
                BlockNode::Paragraph(crate::Paragraph {
                    id: paragraph.id,
                    content: left,
                }),
                BlockNode::Paragraph(crate::Paragraph {
                    id: right_id,
                    content: right,
                }),
            )
        }
        BlockNode::Heading(heading) => {
            let (left, right) = heading.content.split_at(heading.id, offset)?;
            (
                BlockNode::Heading(crate::Heading {
                    id: heading.id,
                    level: heading.level,
                    content: left,
                }),
                BlockNode::Paragraph(crate::Paragraph {
                    id: right_id,
                    content: right,
                }),
            )
        }
        _ => return Err(PositionError::NotText(block.id()).into()),
    })
}

fn allocate_from(next_id: &mut u64) -> NodeId {
    let id = NodeId::new_unchecked(*next_id);
    *next_id += 1;
    id
}

fn toggle_inline_selection(
    state: &mut SnapshotState,
    selection: &TextSelection,
    format: &InlineFormat,
) -> Result<bool, DocumentError> {
    let mut node_ids = Vec::new();
    collect_editable_node_ids(&state.blocks, &mut node_ids);
    let anchor_index = node_ids
        .iter()
        .position(|id| *id == selection.anchor.node_id)
        .ok_or(PositionError::UnknownNode(selection.anchor.node_id))?;
    let head_index = node_ids
        .iter()
        .position(|id| *id == selection.head.node_id)
        .ok_or(PositionError::UnknownNode(selection.head.node_id))?;
    let (start_index, start_offset, end_index, end_offset) =
        if (anchor_index, selection.anchor.text_offset) <= (head_index, selection.head.text_offset)
        {
            (
                anchor_index,
                selection.anchor.text_offset,
                head_index,
                selection.head.text_offset,
            )
        } else {
            (
                head_index,
                selection.head.text_offset,
                anchor_index,
                selection.anchor.text_offset,
            )
        };
    let style = format.as_style();
    let mut ranges = Vec::new();
    let mut covered = 0;
    let mut total = 0;
    for (relative, node_id) in node_ids[start_index..=end_index].iter().enumerate() {
        let node =
            find_node(&state.blocks, *node_id).ok_or(PositionError::UnknownNode(*node_id))?;
        let rich_text = node.text().ok_or(PositionError::NotText(*node_id))?;
        let start = if relative == 0 { start_offset } else { 0 };
        let end = if start_index + relative == end_index {
            end_offset
        } else {
            rich_text.len()
        };
        rich_text.validate_range(*node_id, &(start..end))?;
        let coverage = rich_text.style_coverage(&(start..end), &style);
        covered += coverage.0;
        total += coverage.1;
        ranges.push((*node_id, start..end));
    }
    if total == 0 {
        return Ok(false);
    }
    let enable = covered != total;
    for (node_id, range) in ranges {
        mutate_node(&mut state.blocks, node_id, &mut |node| {
            node.text_mut()
                .ok_or(PositionError::NotText(node_id))?
                .set_style(node_id, range.clone(), style.clone(), enable)
        })?;
        state.dirty_nodes.insert(node_id);
    }
    Ok(true)
}

fn set_link_selection(
    state: &mut SnapshotState,
    selection: &TextSelection,
    target: Option<&str>,
) -> Result<bool, DocumentError> {
    let mut node_ids = Vec::new();
    collect_editable_node_ids(&state.blocks, &mut node_ids);
    let anchor_index = node_ids
        .iter()
        .position(|id| *id == selection.anchor.node_id)
        .ok_or(PositionError::UnknownNode(selection.anchor.node_id))?;
    let head_index = node_ids
        .iter()
        .position(|id| *id == selection.head.node_id)
        .ok_or(PositionError::UnknownNode(selection.head.node_id))?;
    let (start_index, start_offset, end_index, end_offset) =
        if (anchor_index, selection.anchor.text_offset) <= (head_index, selection.head.text_offset)
        {
            (
                anchor_index,
                selection.anchor.text_offset,
                head_index,
                selection.head.text_offset,
            )
        } else {
            (
                head_index,
                selection.head.text_offset,
                anchor_index,
                selection.anchor.text_offset,
            )
        };
    let mut ranges = Vec::new();
    for (relative, node_id) in node_ids[start_index..=end_index].iter().enumerate() {
        let node =
            find_node(&state.blocks, *node_id).ok_or(PositionError::UnknownNode(*node_id))?;
        let rich_text = node.text().ok_or(PositionError::NotText(*node_id))?;
        let start = if relative == 0 { start_offset } else { 0 };
        let end = if start_index + relative == end_index {
            end_offset
        } else {
            rich_text.len()
        };
        rich_text.validate_range(*node_id, &(start..end))?;
        if start < end {
            ranges.push((*node_id, start..end));
        }
    }
    if ranges.is_empty() {
        return Ok(false);
    }
    for (node_id, range) in ranges {
        mutate_node(&mut state.blocks, node_id, &mut |node| {
            if let BlockNode::Image(image) = node {
                image.link = target.map(|target| crate::ImageLink {
                    target: crate::LinkTarget(target.to_owned()),
                    title: image.link.as_ref().and_then(|link| link.title.clone()),
                });
                return Ok(());
            }
            node.text_mut()
                .ok_or(PositionError::NotText(node_id))?
                .set_link(node_id, range.clone(), target)
        })?;
        state.dirty_nodes.insert(node_id);
    }
    Ok(true)
}

fn collect_editable_node_ids(blocks: &BlockSequence, output: &mut Vec<NodeId>) {
    for block in blocks {
        if block.text().is_some() {
            output.push(block.id());
        }
        match block.as_ref() {
            BlockNode::List(list) => {
                for item in list.items.iter() {
                    collect_editable_node_ids(&item.blocks, output);
                }
            }
            BlockNode::BlockQuote { blocks, .. }
            | BlockNode::Alert { blocks, .. }
            | BlockNode::FootnoteDefinition { blocks, .. } => {
                collect_editable_node_ids(blocks, output);
            }
            BlockNode::Table(table) => {
                for row in table.rows.iter() {
                    for cell in row.cells.iter() {
                        collect_editable_node_ids(&cell.blocks, output);
                    }
                }
            }
            _ => {}
        }
    }
}

fn reconcile_selection(
    before: &DocumentSnapshot,
    state: &mut SnapshotState,
    mutation: SelectionMutation,
) -> Result<(), DocumentError> {
    if matches!(mutation, SelectionMutation::ExplicitTextSelection) {
        return validate_selection_in_blocks(&state.blocks, &state.selection);
    }

    if let Selection::Table(selection) = &mut state.selection {
        match mutation {
            SelectionMutation::InsertRow { table_id, index } if selection.table_id == table_id => {
                selection.anchor_row = inserted_index(selection.anchor_row, index);
                selection.head_row = inserted_index(selection.head_row, index);
            }
            SelectionMutation::DeleteRow { table_id, index } if selection.table_id == table_id => {
                selection.anchor_row = deleted_index(selection.anchor_row, index);
                selection.head_row = deleted_index(selection.head_row, index);
            }
            SelectionMutation::MoveRow { table_id, from, to } if selection.table_id == table_id => {
                selection.anchor_row = moved_index(selection.anchor_row, from, to);
                selection.head_row = moved_index(selection.head_row, from, to);
            }
            SelectionMutation::InsertColumn { table_id, index }
                if selection.table_id == table_id =>
            {
                selection.anchor_column = inserted_index(selection.anchor_column, index);
                selection.head_column = inserted_index(selection.head_column, index);
            }
            SelectionMutation::DeleteColumn { table_id, index }
                if selection.table_id == table_id =>
            {
                selection.anchor_column = deleted_index(selection.anchor_column, index);
                selection.head_column = deleted_index(selection.head_column, index);
            }
            SelectionMutation::MoveColumn { table_id, from, to }
                if selection.table_id == table_id =>
            {
                selection.anchor_column = moved_index(selection.anchor_column, from, to);
                selection.head_column = moved_index(selection.head_column, from, to);
            }
            _ => {}
        }
        if let Some(BlockNode::Table(table)) = find_node(&state.blocks, selection.table_id) {
            let last_row = table.row_count().saturating_sub(1);
            let last_column = table.column_count().saturating_sub(1);
            selection.anchor_row = selection.anchor_row.min(last_row);
            selection.head_row = selection.head_row.min(last_row);
            selection.anchor_column = selection.anchor_column.min(last_column);
            selection.head_column = selection.head_column.min(last_column);
        }
    }

    if validate_selection_in_blocks(&state.blocks, &state.selection).is_ok() {
        return Ok(());
    }

    let repaired =
        match state.selection.clone() {
            Selection::Text(selection) => Selection::Text(TextSelection {
                anchor: repair_position(before, &state.blocks, selection.anchor),
                head: repair_position(before, &state.blocks, selection.head),
            }),
            Selection::Table(_) => Selection::Text(TextSelection::caret(
                first_editable_position_in_sequence(&state.blocks).unwrap_or(
                    DocumentPosition::new(NodeId::new_unchecked(1), 0, crate::Affinity::Downstream),
                ),
            )),
        };
    state.selection = repaired;

    if validate_selection_in_blocks(&state.blocks, &state.selection).is_err() {
        let id = allocate_node_id(state);
        let mut blocks = state.blocks.to_vec();
        let caret = Arc::new(BlockNode::Paragraph(crate::Paragraph {
            id,
            content: crate::RichText::default(),
        }));
        blocks.push(caret.clone());
        state.transient_caret = Some(caret);
        state.blocks = BlockSequence::new(blocks);
        state.structure_changed = true;
        state.selection = Selection::Text(TextSelection::caret(DocumentPosition::new(
            id,
            0,
            crate::Affinity::Downstream,
        )));
    }
    validate_selection_in_blocks(&state.blocks, &state.selection)
}

fn inserted_index(value: usize, inserted: usize) -> usize {
    value + usize::from(value >= inserted)
}

fn deleted_index(value: usize, deleted: usize) -> usize {
    if value > deleted { value - 1 } else { value }
}

fn moved_index(value: usize, from: usize, to: usize) -> usize {
    if value == from {
        to
    } else if from < to && value > from && value <= to {
        value - 1
    } else if to < from && value >= to && value < from {
        value + 1
    } else {
        value
    }
}

fn repair_position(
    before: &DocumentSnapshot,
    blocks: &BlockSequence,
    position: DocumentPosition,
) -> DocumentPosition {
    if let Some(text) = find_node(blocks, position.node_id).and_then(BlockNode::text) {
        return clamp_position(position, text);
    }

    if let Some((table_id, row, column)) = before.table_cell_containing(position.node_id)
        && let Some(BlockNode::Table(table)) = find_node(blocks, table_id)
        && let Some(cell) = table
            .rows
            .get(row.min(table.row_count().saturating_sub(1)))
            .and_then(|row| {
                row.cells
                    .get(column.min(table.column_count().saturating_sub(1)))
            })
        && let Some(replacement) = first_editable_position_in_sequence(&cell.blocks)
    {
        return replacement;
    }

    let mut before_ids = Vec::new();
    collect_editable_node_ids(before.blocks(), &mut before_ids);
    if let Some(index) = before_ids.iter().position(|id| *id == position.node_id) {
        if let Some(replacement) = before_ids[index + 1..]
            .iter()
            .find_map(|id| find_node(blocks, *id).and_then(first_editable_in_block))
        {
            return replacement;
        }
        if let Some(replacement) = before_ids[..index]
            .iter()
            .rev()
            .find_map(|id| find_node(blocks, *id).and_then(last_editable_in_block))
        {
            return replacement;
        }
    }
    first_editable_position_in_sequence(blocks).unwrap_or(position)
}

fn clamp_position(position: DocumentPosition, text: &crate::RichText) -> DocumentPosition {
    let source = text.as_string();
    let mut offset = position.text_offset.min(source.len());
    while !source.is_char_boundary(offset) {
        offset = offset.saturating_sub(1);
    }
    DocumentPosition::new(position.node_id, offset, position.affinity)
}

fn first_editable_position_in_sequence(blocks: &BlockSequence) -> Option<DocumentPosition> {
    blocks
        .iter()
        .find_map(|block| first_editable_in_block(block))
}

fn validate_selection(
    snapshot: &DocumentSnapshot,
    selection: &Selection,
) -> Result<(), DocumentError> {
    validate_selection_in_blocks(snapshot.blocks(), selection)
}

fn validate_selection_in_blocks(
    blocks: &BlockSequence,
    selection: &Selection,
) -> Result<(), DocumentError> {
    match selection {
        Selection::Text(selection) => {
            for position in [selection.anchor, selection.head] {
                let node = find_node(blocks, position.node_id)
                    .ok_or(PositionError::UnknownNode(position.node_id))?;
                let text = node
                    .text()
                    .ok_or(PositionError::NotText(position.node_id))?;
                text.validate_range(
                    position.node_id,
                    &(position.text_offset..position.text_offset),
                )?;
            }
        }
        Selection::Table(rectangle) => {
            let Some(BlockNode::Table(table)) = find_node(blocks, rectangle.table_id) else {
                return Err(DocumentError::NotTable(rectangle.table_id));
            };
            let (rows, columns) = rectangle.normalized();
            if *rows.end() >= table.row_count() || *columns.end() >= table.column_count() {
                return Err(DocumentError::TableCoordinate {
                    row: *rows.end(),
                    column: *columns.end(),
                });
            }
        }
    }
    Ok(())
}

fn validate_tree(blocks: &BlockSequence) -> Result<(), DocumentError> {
    let mut ids = HashSet::new();
    validate_sequence(blocks, &mut ids)
}

fn validate_sequence(
    blocks: &BlockSequence,
    ids: &mut HashSet<NodeId>,
) -> Result<(), DocumentError> {
    for block in blocks {
        if !ids.insert(block.id()) {
            return Err(DocumentError::Markdown(format!(
                "duplicate stable node ID {}",
                block.id()
            )));
        }
        match block.as_ref() {
            BlockNode::List(list) => {
                for item in list.items.iter() {
                    if !ids.insert(item.id) {
                        return Err(DocumentError::Markdown(format!(
                            "duplicate list item ID {}",
                            item.id
                        )));
                    }
                    validate_sequence(&item.blocks, ids)?;
                }
            }
            BlockNode::BlockQuote { blocks, .. }
            | BlockNode::Alert { blocks, .. }
            | BlockNode::FootnoteDefinition { blocks, .. } => validate_sequence(blocks, ids)?,
            BlockNode::Table(table) => {
                if table.rows.is_empty() || table.columns.is_empty() {
                    return Err(DocumentError::EmptyTable);
                }
                for row in table.rows.iter() {
                    if row.cells.len() != table.columns.len() || !ids.insert(row.id) {
                        return Err(DocumentError::Markdown(
                            "table row shape or ID invariant violated".into(),
                        ));
                    }
                    for cell in row.cells.iter() {
                        if !ids.insert(cell.id) {
                            return Err(DocumentError::Markdown(format!(
                                "duplicate table cell ID {}",
                                cell.id
                            )));
                        }
                        validate_sequence(&cell.blocks, ids)?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn find_node(blocks: &BlockSequence, id: NodeId) -> Option<&BlockNode> {
    let index = blocks.top_index_containing(id)?;
    let block = blocks.get(index)?.as_ref();
    if block.id() == id {
        return Some(block);
    }
    match block {
        BlockNode::List(list) => list
            .items
            .iter()
            .find_map(|item| find_node(&item.blocks, id)),
        BlockNode::BlockQuote { blocks, .. }
        | BlockNode::Alert { blocks, .. }
        | BlockNode::FootnoteDefinition { blocks, .. } => find_node(blocks, id),
        BlockNode::Table(table) => table
            .rows
            .iter()
            .flat_map(|row| row.cells.iter())
            .find_map(|cell| find_node(&cell.blocks, id)),
        _ => None,
    }
}

fn mutate_node(
    blocks: &mut BlockSequence,
    id: NodeId,
    operation: &mut dyn FnMut(&mut BlockNode) -> Result<(), DocumentError>,
) -> Result<(), DocumentError> {
    let index = blocks
        .top_index_containing(id)
        .ok_or(PositionError::UnknownNode(id))?;
    let mut changed = blocks
        .get(index)
        .expect("the index came from this sequence")
        .clone();
    if changed.id() == id {
        operation(Arc::make_mut(&mut changed))?;
    } else if !mutate_descendant(Arc::make_mut(&mut changed), id, operation)? {
        return Err(PositionError::UnknownNode(id).into());
    }
    *blocks = blocks
        .replacing(index, changed)
        .expect("the index came from this sequence");
    Ok(())
}

fn mutate_descendant(
    block: &mut BlockNode,
    id: NodeId,
    operation: &mut dyn FnMut(&mut BlockNode) -> Result<(), DocumentError>,
) -> Result<bool, DocumentError> {
    match block {
        BlockNode::List(list) => {
            let mut items = list.items.to_vec();
            for item in &mut items {
                if mutate_node_optional(&mut item.blocks, id, operation)? {
                    list.items = items.into();
                    return Ok(true);
                }
            }
        }
        BlockNode::BlockQuote { blocks, .. }
        | BlockNode::Alert { blocks, .. }
        | BlockNode::FootnoteDefinition { blocks, .. } => {
            return mutate_node_optional(blocks, id, operation);
        }
        BlockNode::Table(table) => {
            let mut rows = table.rows.to_vec();
            for row in &mut rows {
                let mut cells = row.cells.to_vec();
                for cell in &mut cells {
                    if mutate_node_optional(&mut cell.blocks, id, operation)? {
                        row.cells = cells.into();
                        table.rows = rows.into();
                        return Ok(true);
                    }
                }
            }
        }
        _ => {}
    }
    Ok(false)
}

fn mutate_node_optional(
    blocks: &mut BlockSequence,
    id: NodeId,
    operation: &mut dyn FnMut(&mut BlockNode) -> Result<(), DocumentError>,
) -> Result<bool, DocumentError> {
    let Some(index) = blocks.top_index_containing(id) else {
        return Ok(false);
    };
    let mut changed = blocks
        .get(index)
        .expect("the index came from this sequence")
        .clone();
    if changed.id() == id {
        operation(Arc::make_mut(&mut changed))?;
    } else if !mutate_descendant(Arc::make_mut(&mut changed), id, operation)? {
        return Ok(false);
    }
    *blocks = blocks
        .replacing(index, changed)
        .expect("the index came from this sequence");
    Ok(true)
}

fn mutate_list_item(blocks: &mut BlockSequence, item_id: NodeId) -> Result<(), DocumentError> {
    let index = blocks
        .top_index_containing(item_id)
        .ok_or(PositionError::UnknownNode(item_id))?;
    let mut changed = blocks
        .get(index)
        .expect("the index came from this sequence")
        .clone();
    if !mutate_list_item_in_block(Arc::make_mut(&mut changed), item_id)? {
        return Err(PositionError::UnknownNode(item_id).into());
    }
    *blocks = blocks
        .replacing(index, changed)
        .expect("the index came from this sequence");
    Ok(())
}

fn mutate_list_item_in_block(
    block: &mut BlockNode,
    item_id: NodeId,
) -> Result<bool, DocumentError> {
    match block {
        BlockNode::List(list) => {
            let mut items = list.items.to_vec();
            if let Some(item) = items.iter_mut().find(|item| item.id == item_id) {
                item.checked = Some(!item.checked.unwrap_or(false));
                list.items = items.into();
                return Ok(true);
            }
            if let Some(item) = items
                .iter_mut()
                .find(|item| item.blocks.contains_node(item_id))
            {
                mutate_list_item(&mut item.blocks, item_id)?;
                list.items = items.into();
                return Ok(true);
            }
        }
        BlockNode::BlockQuote { blocks, .. }
        | BlockNode::Alert { blocks, .. }
        | BlockNode::FootnoteDefinition { blocks, .. } => {
            if blocks.contains_node(item_id) {
                mutate_list_item(blocks, item_id)?;
                return Ok(true);
            }
        }
        BlockNode::Table(table) => {
            let mut rows = table.rows.to_vec();
            for row in &mut rows {
                let mut cells = row.cells.to_vec();
                if let Some(cell) = cells
                    .iter_mut()
                    .find(|cell| cell.blocks.contains_node(item_id))
                {
                    mutate_list_item(&mut cell.blocks, item_id)?;
                    row.cells = cells.into();
                    table.rows = rows.into();
                    return Ok(true);
                }
            }
        }
        _ => {}
    }
    Ok(false)
}

pub(crate) fn snapshot_from_import(
    blocks: BlockSequence,
    selection: Selection,
    source: SourceSpine,
    next_node_id: u64,
    transient_caret: Option<Arc<BlockNode>>,
) -> DocumentSnapshot {
    DocumentSnapshot(Arc::new(SnapshotState {
        revision: Revision::default(),
        blocks,
        selection,
        dirty_nodes: BTreeSet::new(),
        structure_changed: false,
        source,
        next_node_id,
        node_revisions: Arc::new(HashMap::new()),
        transient_caret,
    }))
}

fn changed_node_ids(before: &BlockSequence, after: &BlockSequence) -> BTreeSet<NodeId> {
    let mut changed = BTreeSet::new();
    collect_changed_sequence(before, after, &mut changed);
    changed
}

fn collect_changed_sequence(
    before: &BlockSequence,
    after: &BlockSequence,
    changed: &mut BTreeSet<NodeId>,
) {
    let before_by_id: HashMap<_, _> = before.iter().map(|block| (block.id(), block)).collect();
    let after_ids: HashSet<_> = after.iter().map(|block| block.id()).collect();
    for old in before {
        if !after_ids.contains(&old.id()) {
            collect_block_ids(old, &mut |id| {
                changed.insert(id);
            });
        }
    }
    for current in after {
        let Some(previous) = before_by_id.get(&current.id()) else {
            collect_block_ids(current, &mut |id| {
                changed.insert(id);
            });
            continue;
        };
        if Arc::ptr_eq(previous, current) {
            continue;
        }
        changed.insert(current.id());
        match (previous.as_ref(), current.as_ref()) {
            (BlockNode::List(old), BlockNode::List(new)) => {
                for item in new.items.iter() {
                    if let Some(old_item) =
                        old.items.iter().find(|candidate| candidate.id == item.id)
                    {
                        collect_changed_sequence(&old_item.blocks, &item.blocks, changed);
                    } else {
                        collect_node_ids(&item.blocks, &mut |id| {
                            changed.insert(id);
                        });
                    }
                }
            }
            (
                BlockNode::BlockQuote { blocks: old, .. },
                BlockNode::BlockQuote { blocks: new, .. },
            )
            | (BlockNode::Alert { blocks: old, .. }, BlockNode::Alert { blocks: new, .. })
            | (
                BlockNode::FootnoteDefinition { blocks: old, .. },
                BlockNode::FootnoteDefinition { blocks: new, .. },
            ) => collect_changed_sequence(old, new, changed),
            (BlockNode::Table(old), BlockNode::Table(new)) => {
                for row in new.rows.iter() {
                    let Some(old_row) = old.rows.iter().find(|candidate| candidate.id == row.id)
                    else {
                        for cell in row.cells.iter() {
                            changed.insert(cell.id);
                            collect_node_ids(&cell.blocks, &mut |id| {
                                changed.insert(id);
                            });
                        }
                        continue;
                    };
                    for cell in row.cells.iter() {
                        if let Some(old_cell) = old_row
                            .cells
                            .iter()
                            .find(|candidate| candidate.id == cell.id)
                        {
                            collect_changed_sequence(&old_cell.blocks, &cell.blocks, changed);
                        } else {
                            changed.insert(cell.id);
                            collect_node_ids(&cell.blocks, &mut |id| {
                                changed.insert(id);
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn collect_node_ids(blocks: &BlockSequence, visitor: &mut impl FnMut(NodeId)) {
    for block in blocks {
        collect_block_ids(block, visitor);
    }
}

fn collect_block_ids(block: &BlockNode, visitor: &mut impl FnMut(NodeId)) {
    visitor(block.id());
    match block {
        BlockNode::List(list) => {
            for item in list.items.iter() {
                visitor(item.id);
                collect_node_ids(&item.blocks, visitor);
            }
        }
        BlockNode::BlockQuote { blocks, .. }
        | BlockNode::Alert { blocks, .. }
        | BlockNode::FootnoteDefinition { blocks, .. } => collect_node_ids(blocks, visitor),
        BlockNode::Table(table) => {
            for row in table.rows.iter() {
                visitor(row.id);
                for cell in row.cells.iter() {
                    visitor(cell.id);
                    collect_node_ids(&cell.blocks, visitor);
                }
            }
        }
        _ => {}
    }
}

pub(crate) fn structure_changed(snapshot: &DocumentSnapshot) -> bool {
    snapshot.0.structure_changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Affinity;

    #[test]
    fn cross_block_replacement_joins_top_level_text_and_undo_restores_selection() {
        let source = "**first** end\n\nstart *second*\n";
        let mut document = Document::from_markdown(source).expect("document");
        let before = document.snapshot();
        let first = before.blocks().get(0).expect("first block").id();
        let second = before.blocks().get(1).expect("second block").id();
        let selection = TextSelection {
            anchor: DocumentPosition::new(first, "first".len(), Affinity::Downstream),
            head: DocumentPosition::new(second, "start ".len(), Affinity::Upstream),
        };
        document
            .apply(EditCommand::SetSelection(Selection::Text(
                selection.clone(),
            )))
            .expect("selection");
        document
            .apply(EditCommand::ReplaceSelection {
                text: " + ".into(),
                typing: false,
            })
            .expect("cross-block edit");

        let after = document.snapshot();
        assert_eq!(after.blocks().len(), 1);
        let text = after
            .blocks()
            .get(0)
            .expect("remaining block")
            .text()
            .expect("text");
        assert_eq!(text.as_string(), "first + second");
        assert!(text.runs()[0].styles.contains(&crate::InlineStyle::Bold));
        assert!(
            text.runs()
                .last()
                .expect("suffix")
                .styles
                .contains(&crate::InlineStyle::Italic)
        );

        let restored = document.undo().expect("undo");
        assert_eq!(restored.blocks().len(), 2);
        assert_eq!(restored.selection(), &Selection::Text(selection));
        assert_eq!(restored.serialize().expect("serialize"), source);
    }

    #[test]
    fn stale_worker_result_is_rejected_after_edit_and_undo() {
        let mut document = Document::from_markdown("hello").expect("document");
        let stale_revision = document.snapshot().revision();
        document
            .apply(EditCommand::ReplaceSelection {
                text: "x".into(),
                typing: true,
            })
            .expect("edit");
        document.undo().expect("undo");
        let error = document
            .accept_worker_result(RevisionTagged {
                revision: stale_revision,
                value: (),
            })
            .expect_err("old revision is stale even when content was restored");
        assert!(matches!(error, DocumentError::StaleWorkerResult { .. }));
    }

    #[test]
    fn nested_text_edits_revision_the_leaf_and_enclosing_layout_blocks() {
        let mut document = Document::from_markdown("- outer\n  - inner\n").expect("document");
        let before = document.snapshot();
        let BlockNode::List(outer) = before.blocks().get(0).expect("outer list").as_ref() else {
            panic!("outer list");
        };
        let BlockNode::List(inner) = outer.items[0].blocks.get(1).expect("nested list").as_ref()
        else {
            panic!("nested list");
        };
        let leaf = inner.items[0].blocks.get(0).expect("inner text").id();
        let outer_id = outer.id;
        let inner_id = inner.id;

        assert_eq!(before.node_revision(leaf), Some(Revision::default()));
        assert_eq!(
            before.node_revision(NodeId::new_unchecked(u64::MAX)),
            None,
            "unknown IDs must not masquerade as unchanged nodes"
        );

        let result = document
            .apply(EditCommand::ReplaceText {
                node_id: leaf,
                range: 5..5,
                text: " edited".into(),
                selection_after: None,
                typing: true,
            })
            .expect("nested edit");

        assert!(result.dirty_node_ids.contains(&leaf));
        assert!(result.dirty_node_ids.contains(&inner_id));
        assert!(result.dirty_node_ids.contains(&outer_id));
        assert_eq!(result.text_changed_node, Some(leaf));
        assert_eq!(result.snapshot.node_revision(leaf), Some(result.revision));
        assert_eq!(
            result.snapshot.node_revision(outer_id),
            Some(result.revision),
            "the top-level layout cache key must advance for nested edits"
        );
    }

    #[test]
    fn large_top_level_text_edit_keeps_unrelated_block_allocations_shared() {
        let source = (0..1_025)
            .map(|index| format!("paragraph {index}"))
            .collect::<Vec<_>>()
            .join("\n\n");
        let mut document = Document::from_markdown(source).expect("large document");
        let before = document.snapshot();
        let last = before.blocks().get(1_024).expect("last block");
        let node_id = last.id();
        let offset = last.text().expect("last text").len();

        let result = document
            .apply(EditCommand::ReplaceText {
                node_id,
                range: offset..offset,
                text: "!".into(),
                selection_after: None,
                typing: true,
            })
            .expect("localized edit");

        assert_eq!(result.dirty_node_ids, BTreeSet::from([node_id]));
        for index in [0, 255, 256, 511, 512, 767] {
            assert!(Arc::ptr_eq(
                before.blocks().get(index).expect("before block"),
                result.snapshot.blocks().get(index).expect("after block")
            ));
        }
        assert!(!Arc::ptr_eq(
            before.blocks().get(1_024).expect("before target"),
            result.snapshot.blocks().get(1_024).expect("after target")
        ));
    }

    #[test]
    fn task_toggle_keeps_unrelated_top_level_allocations_and_source_shared() {
        let source = "Title\n=====\n\n- [ ] task\n";
        let mut document = Document::from_markdown(source).expect("document");
        let before = document.snapshot();
        let BlockNode::List(list) = before.blocks().get(1).expect("task list").as_ref() else {
            panic!("task list");
        };
        let item_id = list.items[0].id;
        document
            .apply(EditCommand::ToggleTask { item_id })
            .expect("toggle task");
        let after = document.snapshot();
        assert!(Arc::ptr_eq(
            before.blocks().get(0).expect("before heading"),
            after.blocks().get(0).expect("after heading")
        ));
        assert!(
            after
                .serialize()
                .expect("serialize")
                .starts_with("Title\n=====\n")
        );
    }

    #[test]
    fn inline_formatting_is_one_transaction_across_blocks() {
        let mut document = Document::from_markdown("one\n\ntwo").expect("document");
        let snapshot = document.snapshot();
        let first = snapshot.blocks().get(0).expect("first").id();
        let second = snapshot.blocks().get(1).expect("second").id();
        document
            .apply(EditCommand::SetSelection(Selection::Text(TextSelection {
                anchor: DocumentPosition::new(first, 1, Affinity::Downstream),
                head: DocumentPosition::new(second, 2, Affinity::Upstream),
            })))
            .expect("selection");
        document
            .apply(EditCommand::ToggleInlineSelection {
                format: InlineFormat::Bold,
            })
            .expect("format");
        let formatted = document.snapshot();
        for block in formatted.blocks() {
            assert!(
                block
                    .text()
                    .expect("text")
                    .runs()
                    .iter()
                    .any(|run| run.styles.contains(&crate::InlineStyle::Bold))
            );
        }
        let restored = document.undo().expect("single undo");
        assert_eq!(restored.serialize().expect("serialize"), "one\n\ntwo");
    }

    #[test]
    fn insertion_registry_allocates_valid_rich_blocks() {
        let mut document = Document::from_markdown("before").expect("document");
        for kind in [
            InsertBlockKind::Heading(2),
            InsertBlockKind::OrderedList,
            InsertBlockKind::UnorderedList,
            InsertBlockKind::TaskList,
            InsertBlockKind::BlockQuote,
            InsertBlockKind::CodeBlock,
            InsertBlockKind::Image,
            InsertBlockKind::Table,
            InsertBlockKind::ThematicBreak,
            InsertBlockKind::Paragraph,
        ] {
            document
                .apply(EditCommand::InsertBlockAfterSelection { kind })
                .expect("inserted block preserves tree invariants");
        }
        let snapshot = document.snapshot();
        validate_tree(snapshot.blocks()).expect("valid stable IDs");
        let source = snapshot.serialize().expect("serialize");
        assert!(source.contains("|  |  |"));
        assert!(
            source.contains("1. "),
            "numbered-list command stays ordered"
        );
    }

    #[test]
    fn enter_splits_paragraphs_list_items_and_table_cell_blocks() {
        let mut paragraph = Document::from_markdown("hello").expect("paragraph");
        let paragraph_id = paragraph.snapshot().blocks().get(0).expect("block").id();
        paragraph
            .apply(EditCommand::SetSelection(Selection::Text(
                TextSelection::caret(DocumentPosition::new(paragraph_id, 2, Affinity::Downstream)),
            )))
            .expect("caret");
        let split = paragraph
            .apply(EditCommand::SplitSelection)
            .expect("split paragraph");
        assert_eq!(split.text_changed_node, None);
        assert_eq!(paragraph.snapshot().blocks().len(), 2);

        let mut list = Document::from_markdown("- one").expect("list");
        let list_snapshot = list.snapshot();
        let BlockNode::List(list_block) = list_snapshot.blocks().get(0).expect("list").as_ref()
        else {
            panic!("list");
        };
        let list_text = list_block.items[0].blocks.get(0).expect("item").id();
        list.apply(EditCommand::SetSelection(Selection::Text(
            TextSelection::caret(DocumentPosition::new(list_text, 1, Affinity::Downstream)),
        )))
        .expect("caret");
        list.apply(EditCommand::SplitSelection)
            .expect("split list item");
        let list_snapshot = list.snapshot();
        let BlockNode::List(list_block) = list_snapshot.blocks().get(0).expect("list").as_ref()
        else {
            panic!("list");
        };
        assert_eq!(list_block.items.len(), 2);

        let mut table = Document::from_markdown("before").expect("table document");
        table
            .apply(EditCommand::InsertBlockAfterSelection {
                kind: InsertBlockKind::Table,
            })
            .expect("insert table");
        table
            .apply(EditCommand::SplitSelection)
            .expect("insert paragraph in cell");
        let table_snapshot = table.snapshot();
        let BlockNode::Table(table_block) = table_snapshot.blocks().get(1).expect("table").as_ref()
        else {
            panic!("table");
        };
        assert_eq!(table_block.rows[0].cells[0].blocks.len(), 2);
    }

    #[test]
    fn enter_on_empty_list_item_exits_the_list() {
        let mut document = Document::from_markdown("before").expect("document");
        document
            .apply(EditCommand::InsertBlockAfterSelection {
                kind: InsertBlockKind::UnorderedList,
            })
            .expect("insert list");
        document
            .apply(EditCommand::SplitSelection)
            .expect("exit empty item");
        assert!(
            document
                .snapshot()
                .blocks()
                .iter()
                .all(|block| !matches!(block.as_ref(), BlockNode::List(_)))
        );
        assert!(
            document
                .snapshot()
                .validates_position(match document.snapshot().selection() {
                    Selection::Text(selection) => selection.head,
                    Selection::Table(_) => panic!("text selection"),
                })
        );
    }

    #[test]
    fn list_items_indent_and_outdent_without_changing_editable_node_ids() {
        let mut document = Document::from_markdown("- one\n- two").expect("list");
        let snapshot = document.snapshot();
        let BlockNode::List(list) = snapshot.blocks().get(0).expect("list").as_ref() else {
            panic!("list");
        };
        let item_id = list.items[1].id;
        let text_id = list.items[1].blocks.get(0).expect("paragraph").id();
        document
            .apply(EditCommand::IndentListItem { item_id })
            .expect("indent");
        let indented = document.snapshot();
        assert_eq!(
            indented.list_item_containing(text_id).map(|item| item.1),
            Some(item_id)
        );
        let BlockNode::List(list) = indented.blocks().get(0).expect("list").as_ref() else {
            panic!("list");
        };
        assert_eq!(list.items.len(), 1);
        assert!(matches!(
            list.items[0].blocks.iter().next_back().map(Arc::as_ref),
            Some(BlockNode::List(_))
        ));

        document
            .apply(EditCommand::OutdentListItem { item_id })
            .expect("outdent");
        let outdented = document.snapshot();
        let BlockNode::List(list) = outdented.blocks().get(0).expect("list").as_ref() else {
            panic!("list");
        };
        assert_eq!(list.items.len(), 2);
        assert_eq!(list.items[1].id, item_id);
        assert!(outdented.validates_position(DocumentPosition::new(
            text_id,
            0,
            Affinity::Downstream,
        )));
    }

    #[test]
    fn link_destination_replacement_does_not_stack_link_styles() {
        let mut document = Document::from_markdown("link").expect("document");
        let node_id = document.snapshot().blocks().get(0).expect("paragraph").id();
        document
            .apply(EditCommand::SetSelection(Selection::Text(TextSelection {
                anchor: DocumentPosition::new(node_id, 0, Affinity::Downstream),
                head: DocumentPosition::new(node_id, 4, Affinity::Upstream),
            })))
            .expect("selection");
        document
            .apply(EditCommand::SetLinkSelection {
                target: Some("https://one.example".into()),
            })
            .expect("first link");
        document
            .apply(EditCommand::SetLinkSelection {
                target: Some("https://two.example".into()),
            })
            .expect("replace link");
        let snapshot = document.snapshot();
        let links = snapshot
            .node(node_id)
            .and_then(BlockNode::text)
            .expect("text")
            .runs()[0]
            .styles
            .iter()
            .filter(|style| matches!(style, crate::InlineStyle::Link(_)))
            .count();
        assert_eq!(links, 1);
        assert_eq!(
            snapshot.serialize().expect("serialize"),
            "[link](https://two.example)"
        );
    }

    #[test]
    fn image_attributes_are_one_undoable_source_preserving_transaction() {
        let mut document = Document::from_markdown("![old](old.png)").expect("document");
        let image_id = document.snapshot().blocks().get(0).expect("image").id();
        document
            .apply(EditCommand::SetImageAttributes {
                image_id,
                source: "images/new.png".into(),
                alt: "New description".into(),
            })
            .expect("image attributes");
        assert_eq!(
            document.snapshot().serialize().expect("serialize"),
            "![New description](images/new.png)"
        );
        assert_eq!(
            document
                .undo()
                .expect("undo")
                .serialize()
                .expect("serialize"),
            "![old](old.png)"
        );
    }

    #[test]
    fn markdown_paste_preserves_inline_formatting_and_undo() {
        let source = "before after";
        let mut document = Document::from_markdown(source).expect("document");
        let node_id = document.snapshot().blocks().get(0).expect("paragraph").id();
        document
            .apply(EditCommand::SetSelection(Selection::Text(
                TextSelection::caret(DocumentPosition::new(node_id, 7, Affinity::Downstream)),
            )))
            .expect("caret");

        document
            .apply(EditCommand::PasteMarkdown {
                markdown: "**bold**".into(),
            })
            .expect("rich paste");
        let snapshot = document.snapshot();
        let text = snapshot
            .node(node_id)
            .and_then(BlockNode::text)
            .expect("text");
        assert_eq!(text.as_string(), "before boldafter");
        assert!(
            text.runs().iter().any(|run| {
                run.range == (7..11) && run.styles.contains(&crate::InlineStyle::Bold)
            })
        );
        assert_eq!(
            snapshot.serialize().expect("serialize"),
            "before **bold**after"
        );

        assert_eq!(
            document
                .undo()
                .expect("undo")
                .serialize()
                .expect("serialize"),
            source
        );
    }

    #[test]
    fn structural_markdown_paste_remaps_ids_and_is_one_transaction() {
        let source = "before after";
        let mut document = Document::from_markdown(source).expect("document");
        let node_id = document.snapshot().blocks().get(0).expect("paragraph").id();
        document
            .apply(EditCommand::SetSelection(Selection::Text(
                TextSelection::caret(DocumentPosition::new(node_id, 6, Affinity::Downstream)),
            )))
            .expect("caret");
        document
            .apply(EditCommand::PasteMarkdown {
                markdown: "# Heading\n\n- one\n- two".into(),
            })
            .expect("block paste");

        let snapshot = document.snapshot();
        let blocks = snapshot.blocks().to_vec();
        assert_eq!(blocks.len(), 4);
        assert!(matches!(blocks[0].as_ref(), BlockNode::Paragraph(_)));
        assert!(matches!(blocks[1].as_ref(), BlockNode::Heading(_)));
        assert!(matches!(blocks[2].as_ref(), BlockNode::List(_)));
        assert!(matches!(blocks[3].as_ref(), BlockNode::Paragraph(_)));
        let mut ids = Vec::new();
        collect_node_ids(snapshot.blocks(), &mut |id| ids.push(id));
        let unique = ids.iter().copied().collect::<HashSet<_>>();
        assert_eq!(
            ids.len(),
            unique.len(),
            "pasted nodes must receive fresh IDs"
        );

        assert_eq!(
            document
                .undo()
                .expect("one undo")
                .serialize()
                .expect("serialize"),
            source
        );
        assert!(matches!(document.undo(), Err(DocumentError::NothingToUndo)));
    }

    #[test]
    fn table_commands_and_tsv_expansion_are_undoable_transactions() {
        let mut document = Document::from_markdown("before").expect("document");
        document
            .apply(EditCommand::InsertBlockAfterSelection {
                kind: InsertBlockKind::Table,
            })
            .expect("table");
        let snapshot = document.snapshot();
        let BlockNode::Table(table) = snapshot.blocks().get(1).expect("table").as_ref() else {
            panic!("table");
        };
        let table_id = table.id;
        document
            .apply(EditCommand::InsertTableRow { table_id, index: 3 })
            .expect("row");
        document
            .apply(EditCommand::InsertTableColumn { table_id, index: 2 })
            .expect("column");
        document
            .apply(EditCommand::SetTableColumnAlignment {
                table_id,
                column: 1,
                alignment: crate::ColumnAlignment::Right,
            })
            .expect("alignment");
        document
            .apply(EditCommand::SetTableColumnWidth {
                table_id,
                column: 0,
                width: 144.,
            })
            .expect("width");
        document
            .apply(EditCommand::SetTableBorder {
                table_id,
                border: crate::TableBorder::PhysicalPixel,
            })
            .expect("border");
        let before_paste = document.snapshot();
        document
            .apply(EditCommand::PasteTsv {
                table_id,
                row: 3,
                column: 2,
                text: "a\tb\nc\td".into(),
            })
            .expect("expanding paste");
        let expanded = document.snapshot();
        let BlockNode::Table(table) = expanded.node(table_id).expect("table") else {
            panic!("table");
        };
        assert_eq!((table.row_count(), table.column_count()), (5, 4));
        let restored = document.undo().expect("one paste undo");
        assert_eq!(
            restored.node(table_id).map(|node| match node {
                BlockNode::Table(table) => (table.row_count(), table.column_count()),
                _ => (0, 0),
            }),
            before_paste.node(table_id).map(|node| match node {
                BlockNode::Table(table) => (table.row_count(), table.column_count()),
                _ => (0, 0),
            })
        );
    }

    #[test]
    fn deleting_the_active_table_row_repairs_caret_before_publication() {
        let source = "| A | B |\n| --- | --- |\n| one | two |\n| three | four |\n";
        let mut document = Document::from_markdown(source).expect("document");
        let snapshot = document.snapshot();
        let BlockNode::Table(table) = snapshot.blocks().get(0).expect("table").as_ref() else {
            panic!("table");
        };
        let table_id = table.id;
        let active = table.rows[1].cells[1]
            .blocks
            .get(0)
            .expect("active cell")
            .id();
        document
            .apply(EditCommand::SetSelection(Selection::Text(
                TextSelection::caret(DocumentPosition::new(active, 2, Affinity::Downstream)),
            )))
            .expect("selection");
        document
            .apply(EditCommand::DeleteTableRow { table_id, index: 1 })
            .expect("delete row");

        let snapshot = document.snapshot();
        let Selection::Text(selection) = snapshot.selection() else {
            panic!("text selection");
        };
        assert!(snapshot.validates_position(selection.anchor));
        let BlockNode::Table(table) = snapshot.node(table_id).expect("table") else {
            panic!("table");
        };
        let expected = table.rows[1].cells[1]
            .blocks
            .get(0)
            .expect("nearest same-column cell")
            .id();
        assert_eq!(selection.head.node_id, expected);
        document
            .apply(EditCommand::ReplaceSelection {
                text: "edited ".into(),
                typing: true,
            })
            .expect("typing works without another click");
    }

    #[test]
    fn rectangular_selection_tracks_table_structure() {
        let mut document =
            Document::from_markdown("| A | B |\n| --- | --- |\n| one | two |\n| three | four |\n")
                .expect("document");
        let table_id = document.snapshot().blocks().get(0).expect("table").id();
        document
            .apply(EditCommand::SetSelection(Selection::Table(
                crate::RectangularSelection {
                    table_id,
                    anchor_row: 1,
                    anchor_column: 1,
                    head_row: 2,
                    head_column: 0,
                },
            )))
            .expect("rectangular selection");
        document
            .apply(EditCommand::InsertTableRow { table_id, index: 1 })
            .expect("insert row");
        document
            .apply(EditCommand::InsertTableColumn { table_id, index: 0 })
            .expect("insert column");
        let Selection::Table(selection) = document.snapshot().selection().clone() else {
            panic!("table selection");
        };
        assert_eq!(
            (
                selection.anchor_row,
                selection.anchor_column,
                selection.head_row,
                selection.head_column,
            ),
            (2, 2, 3, 1)
        );
        document
            .apply(EditCommand::MoveTableRow {
                table_id,
                from: 3,
                to: 1,
            })
            .expect("move row");
        let Selection::Table(selection) = document.snapshot().selection().clone() else {
            panic!("table selection");
        };
        assert_eq!((selection.anchor_row, selection.head_row), (3, 1));
    }

    #[test]
    fn invalid_explicit_post_edit_selection_is_rejected_atomically() {
        let mut document = Document::from_markdown("old").expect("document");
        let node_id = document.snapshot().blocks().get(0).expect("paragraph").id();
        let invalid = TextSelection::caret(DocumentPosition::new(
            NodeId::new_unchecked(u64::MAX),
            0,
            Affinity::Downstream,
        ));
        let error = document
            .apply(EditCommand::ReplaceText {
                node_id,
                range: 0..3,
                text: "new".into(),
                selection_after: Some(Selection::Text(invalid)),
                typing: false,
            })
            .expect_err("invalid selection must reject the transaction");
        assert!(matches!(
            error,
            DocumentError::Position(PositionError::UnknownNode(_))
        ));
        assert_eq!(document.snapshot().serialize().expect("serialize"), "old");
    }

    #[test]
    fn html_conversion_preserves_a_caret_host_once_authored_even_if_emptied() {
        let source = "<p>First</p>\n";
        let mut document = Document::from_markdown(source).unwrap();
        let html_id = document.snapshot().blocks().get(0).unwrap().id();
        let host_id = document.snapshot().blocks().get(1).unwrap().id();
        document
            .apply(EditCommand::ReplaceSelection {
                text: "Tail".into(),
                typing: false,
            })
            .unwrap();
        document
            .apply(EditCommand::ReplaceText {
                node_id: host_id,
                range: 0..4,
                text: String::new(),
                selection_after: None,
                typing: false,
            })
            .unwrap();
        let authored = document.snapshot();
        assert!(!authored.is_transient_caret(authored.blocks().get(1).unwrap()));
        document
            .apply(EditCommand::ConvertHtmlToMarkdown { node_id: html_id })
            .unwrap();
        let converted = document.snapshot();
        assert_eq!(converted.blocks().len(), 2);
        assert_eq!(converted.blocks().get(1).unwrap().id(), host_id);
        assert!(Arc::ptr_eq(
            converted.blocks().get(1).unwrap(),
            authored.blocks().get(1).unwrap()
        ));
        assert_eq!(converted.serialize().unwrap(), "First\n\n\n");
        document.undo().unwrap();
        assert_eq!(
            document.snapshot().serialize().unwrap(),
            authored.serialize().unwrap()
        );
    }

    #[test]
    fn untouched_caret_host_does_not_export_separators_after_structural_edits() {
        let source = "<!-- exact -->\n\n---\n";
        let mut document = Document::from_markdown(source).unwrap();
        let before = document.snapshot();
        document
            .apply(EditCommand::DeleteBlock {
                node_id: before.blocks().get(1).unwrap().id(),
            })
            .unwrap();
        let after = document.snapshot();
        assert_eq!(
            after.blocks().len(),
            2,
            "the noneditable document still needs a caret host"
        );
        assert!(after.is_transient_caret(after.blocks().get(1).unwrap()));
        assert_eq!(after.serialize().unwrap(), "<!-- exact -->\n");
        assert!(
            matches!(after.selection(), Selection::Text(range) if after.validates_position(range.head))
        );
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }

    #[test]
    fn explicit_html_conversion_has_no_unnecessary_caret_block_or_separator() {
        for ending in ["", "\n", "\r\n", "\n\n\n"] {
            let source = format!("<div><p>First</p><p>Second</p></div>{ending}");
            let mut document = Document::from_markdown(source.as_str()).unwrap();
            let before = document.snapshot();
            let id = before.blocks().get(0).unwrap().id();
            document
                .apply(EditCommand::ConvertHtmlToMarkdown { node_id: id })
                .unwrap();
            let after = document.snapshot();
            let separator = if ending == "\r\n" { "\r\n\r\n" } else { "\n\n" };
            assert_eq!(
                after.serialize().unwrap(),
                format!("First{separator}Second{ending}")
            );
            assert_eq!(after.blocks().len(), 2);
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), source);
            document.redo().unwrap();
            assert_eq!(
                document.snapshot().serialize().unwrap(),
                after.serialize().unwrap()
            );
        }
    }

    #[test]
    fn source_with_no_editable_nodes_gets_a_transient_valid_caret() {
        for source in ["<!-- exact -->\n", "---\n"] {
            let document = Document::from_markdown(source).expect("document");
            let snapshot = document.snapshot();
            let Selection::Text(selection) = snapshot.selection() else {
                panic!("text selection");
            };
            assert!(snapshot.validates_position(selection.head));
            assert_eq!(snapshot.serialize().expect("unchanged source"), source);
        }
    }

    #[test]
    fn newly_inserted_table_descendants_are_immediately_indexed_for_editing() {
        let mut document = Document::from_markdown("before").expect("document");
        document
            .apply(EditCommand::InsertBlockAfterSelection {
                kind: InsertBlockKind::Table,
            })
            .expect("table");
        let snapshot = document.snapshot();
        let BlockNode::Table(table) = snapshot.blocks().get(1).expect("table").as_ref() else {
            panic!("table");
        };
        let table_id = table.id;

        document
            .apply(EditCommand::InsertTableRow { table_id, index: 1 })
            .expect("row");
        document
            .apply(EditCommand::InsertTableColumn { table_id, index: 1 })
            .expect("column");

        let inserted = document.snapshot();
        let BlockNode::Table(table) = inserted.node(table_id).expect("table") else {
            panic!("table");
        };
        let inserted_text_id = table.rows[1].cells[1]
            .blocks
            .get(0)
            .expect("inserted cell text")
            .id();
        assert!(inserted.node(inserted_text_id).is_some());
        assert_eq!(
            inserted.node_revision(inserted_text_id),
            Some(inserted.revision())
        );

        let edited = document
            .apply(EditCommand::ReplaceText {
                node_id: inserted_text_id,
                range: 0..0,
                text: "indexed".into(),
                selection_after: None,
                typing: true,
            })
            .expect("edit newly inserted cell");
        assert_eq!(
            edited
                .snapshot
                .node(inserted_text_id)
                .and_then(BlockNode::text)
                .map(|text| text.as_string())
                .as_deref(),
            Some("indexed")
        );
    }

    #[test]
    fn arbitrary_edit_sequences_preserve_invariants_and_undo_state() {
        fn random(seed: &mut u64) -> u64 {
            *seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            *seed
        }

        fn editable_nodes(snapshot: &DocumentSnapshot) -> Vec<(NodeId, String)> {
            let mut ids = Vec::new();
            collect_node_ids(snapshot.blocks(), &mut |id| ids.push(id));
            ids.into_iter()
                .filter_map(|id| {
                    snapshot
                        .node(id)
                        .and_then(BlockNode::text)
                        .map(|text| (id, text.as_string()))
                })
                .collect()
        }

        let mut document = Document::from_markdown(concat!(
            "# Start\n\n",
            "Paragraph with **formatting** and emoji 🎉.\n\n",
            "- first\n- second\n\n",
            "| A | B |\n| --- | ---: |\n| one | two |\n"
        ))
        .expect("document");
        let mut seed = 0x5eed_cafe_f00d_u64;

        for step in 0..300 {
            let snapshot = document.snapshot();
            validate_tree(snapshot.blocks()).expect("valid tree before operation");
            let editable = editable_nodes(&snapshot);
            let (node_id, text) = &editable[random(&mut seed) as usize % editable.len()];
            let boundaries = text
                .char_indices()
                .map(|(offset, _)| offset)
                .chain(std::iter::once(text.len()))
                .collect::<Vec<_>>();
            let first = boundaries[random(&mut seed) as usize % boundaries.len()];
            let second = boundaries[random(&mut seed) as usize % boundaries.len()];
            let start = first.min(second);
            let end = first.max(second);
            document
                .apply(EditCommand::SetSelection(Selection::Text(TextSelection {
                    anchor: DocumentPosition::new(*node_id, start, Affinity::Downstream),
                    head: DocumentPosition::new(*node_id, end, Affinity::Upstream),
                })))
                .expect("valid randomized selection");

            let before = document.snapshot();
            let before_markdown = before.serialize().expect("serialize before");
            let before_selection = before.selection().clone();
            let table = before
                .blocks()
                .iter()
                .find_map(|block| match block.as_ref() {
                    BlockNode::Table(table) => {
                        Some((table.id, table.row_count(), table.column_count()))
                    }
                    _ => None,
                });
            let command = match random(&mut seed) % 8 {
                0 => EditCommand::ReplaceSelection {
                    text: ["x", "é", "🎉", " two words"][random(&mut seed) as usize % 4].into(),
                    typing: false,
                },
                1 => EditCommand::ToggleInlineSelection {
                    format: [
                        InlineFormat::Bold,
                        InlineFormat::Italic,
                        InlineFormat::Strikethrough,
                        InlineFormat::Code,
                    ][random(&mut seed) as usize % 4]
                        .clone(),
                },
                2 => EditCommand::SplitSelection,
                3 => EditCommand::InsertBlockAfterSelection {
                    kind: [
                        InsertBlockKind::Paragraph,
                        InsertBlockKind::Heading(2),
                        InsertBlockKind::UnorderedList,
                        InsertBlockKind::BlockQuote,
                        InsertBlockKind::CodeBlock,
                        InsertBlockKind::Image,
                        InsertBlockKind::Table,
                        InsertBlockKind::ThematicBreak,
                    ][random(&mut seed) as usize % 8],
                },
                4 => EditCommand::SetBlockStyle {
                    node_id: *node_id,
                    style: [
                        BlockStyle::Paragraph,
                        BlockStyle::Heading(1),
                        BlockStyle::Heading(3),
                        BlockStyle::CodeBlock,
                    ][random(&mut seed) as usize % 4],
                },
                5 => table.map_or(
                    EditCommand::PasteMarkdown {
                        markdown: "**pasted**".into(),
                    },
                    |(table_id, rows, _)| EditCommand::InsertTableRow {
                        table_id,
                        index: random(&mut seed) as usize % (rows + 1),
                    },
                ),
                6 => table.map_or(
                    EditCommand::PasteMarkdown {
                        markdown: "[link](https://example.com)".into(),
                    },
                    |(table_id, _, columns)| EditCommand::InsertTableColumn {
                        table_id,
                        index: random(&mut seed) as usize % (columns + 1),
                    },
                ),
                _ => EditCommand::PasteMarkdown {
                    markdown: ["plain", "*emphasis*", "## heading\n\nbody"]
                        [random(&mut seed) as usize % 3]
                        .into(),
                },
            };

            match document.apply(command) {
                Ok(result) if !result.dirty_node_ids.is_empty() => {
                    validate_tree(result.snapshot.blocks())
                        .unwrap_or_else(|error| panic!("step {step} invalid after edit: {error}"));
                    let restored = document.undo().expect("every mutation is undoable");
                    assert_eq!(
                        restored.serialize().expect("serialize restored"),
                        before_markdown,
                        "step {step} changed bytes after undo"
                    );
                    assert_eq!(
                        restored.selection(),
                        &before_selection,
                        "step {step} changed selection after undo"
                    );
                    let redone = document.redo().expect("redo randomized mutation");
                    validate_tree(redone.blocks()).expect("valid tree after redo");
                }
                Ok(_) => {
                    assert_eq!(
                        document
                            .snapshot()
                            .serialize()
                            .expect("unchanged serialize"),
                        before_markdown
                    );
                }
                Err(_) => {
                    let after_error = document.snapshot();
                    validate_tree(after_error.blocks()).expect("valid tree after rejected command");
                    assert_eq!(
                        after_error.serialize().expect("serialize after error"),
                        before_markdown,
                        "step {step} partially applied a rejected command"
                    );
                    assert_eq!(after_error.selection(), &before_selection);
                }
            }
        }
    }

    #[test]
    fn oversized_source_remains_byte_exact_after_edit_and_undo() {
        let source = format!(
            "---\ntitle: Oversized\n---\n\n{}\n\n<!-- tail -->\n",
            "source-preserving payload 🎉 ".repeat(410_000)
        );
        assert!(source.len() > 10 * 1024 * 1024);
        let mut document = Document::from_markdown(source.clone()).expect("oversized document");
        let node_id = document.snapshot().blocks().get(1).expect("paragraph").id();
        document
            .apply(EditCommand::SetSelection(Selection::Text(
                TextSelection::caret(DocumentPosition::new(node_id, 0, Affinity::Downstream)),
            )))
            .expect("caret");
        document
            .apply(EditCommand::ReplaceSelection {
                text: "edited ".into(),
                typing: false,
            })
            .expect("edit oversized document");

        assert_eq!(
            document
                .undo()
                .expect("undo")
                .serialize()
                .expect("serialize"),
            source
        );
    }
}
