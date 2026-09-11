//! Source-order ranges independent of visual columns and semantic containers.
//!
//! Endpoint paths use the persistent sequence's descendant index. Traversal
//! descends only into boundary containers; untouched subtrees keep their Arcs.

use std::sync::Arc;

use crate::{
    BlockNode, BlockSequence, DocumentError, DocumentPosition, ListKind, NodeId, Paragraph,
    PositionError, RichText, TextSelection,
};

pub(crate) struct TreeRange<'a> {
    pub start: DocumentPosition,
    pub end: DocumentPosition,
    pub start_block: &'a Arc<BlockNode>,
    pub end_block: &'a Arc<BlockNode>,
    start_path: Vec<usize>,
    end_path: Vec<usize>,
}

impl<'a> TreeRange<'a> {
    pub fn resolve(
        blocks: &'a BlockSequence,
        selection: &TextSelection,
    ) -> Result<Self, DocumentError> {
        let (anchor_path, anchor) = endpoint(blocks, selection.anchor)?;
        let (head_path, head) = endpoint(blocks, selection.head)?;
        let (start, start_path, start_block, end, end_path, end_block) =
            if (&anchor_path, selection.anchor.text_offset)
                <= (&head_path, selection.head.text_offset)
            {
                (
                    selection.anchor,
                    anchor_path,
                    anchor,
                    selection.head,
                    head_path,
                    head,
                )
            } else {
                (
                    selection.head,
                    head_path,
                    head,
                    selection.anchor,
                    anchor_path,
                    anchor,
                )
            };
        Ok(Self {
            start,
            end,
            start_block,
            end_block,
            start_path,
            end_path,
        })
    }

    /// Only sibling text blocks share a flow. A term, description, list item,
    /// quotation and table cell each own their own child sequence.
    pub fn same_flow(&self) -> bool {
        self.start_path[..self.start_path.len() - 1] == self.end_path[..self.end_path.len() - 1]
    }

    pub fn extract(&self, blocks: &BlockSequence) -> Result<BlockSequence, DocumentError> {
        Walker {
            range: self,
            operation: Operation::Extract,
        }
        .sequence(blocks, &[])
    }

    /// Replace boundary leaves and remove the selected interior. Endpoint
    /// replacements stay in their authored containers; callers decide whether
    /// sibling text can join. Tables retain their rectangular cell identities.
    pub fn splice(
        &self,
        blocks: &BlockSequence,
        start: &[Arc<BlockNode>],
        end: &[Arc<BlockNode>],
        next_id: &mut u64,
    ) -> Result<BlockSequence, DocumentError> {
        Walker {
            range: self,
            operation: Operation::Splice {
                start,
                end,
                next_id,
            },
        }
        .sequence(blocks, &[])
    }

    fn relation(&self, path: &[usize]) -> Relation {
        if self.start_path.starts_with(path) || self.end_path.starts_with(path) {
            Relation::Boundary
        } else if self.start_path.as_slice() < path && path < self.end_path.as_slice() {
            Relation::Inside
        } else {
            Relation::Outside
        }
    }
}

fn endpoint(
    blocks: &BlockSequence,
    position: DocumentPosition,
) -> Result<(Vec<usize>, &Arc<BlockNode>), DocumentError> {
    let mut path = Vec::new();
    let block = find_path(blocks, position.node_id, &mut path)
        .ok_or(PositionError::UnknownNode(position.node_id))?;
    block
        .text()
        .ok_or(PositionError::NotText(position.node_id))?
        .validate_range(
            position.node_id,
            &(position.text_offset..position.text_offset),
        )?;
    Ok((path, block))
}

fn find_path<'a>(
    blocks: &'a BlockSequence,
    id: NodeId,
    path: &mut Vec<usize>,
) -> Option<&'a Arc<BlockNode>> {
    let index = blocks.top_index_containing(id)?;
    path.push(index);
    let block = blocks.get(index)?;
    if block.id() == id {
        return Some(block);
    }
    match block.as_ref() {
        BlockNode::Definition { blocks, .. }
        | BlockNode::BlockQuote { blocks, .. }
        | BlockNode::Alert { blocks, .. }
        | BlockNode::FootnoteDefinition { blocks, .. } => find_path(blocks, id, path),
        BlockNode::List(list) => {
            let (index, item) = list
                .items
                .iter()
                .enumerate()
                .find(|(_, item)| item.blocks.contains_node(id))?;
            path.push(index);
            find_path(&item.blocks, id, path)
        }
        BlockNode::Table(table) => {
            for (row_index, row) in table.rows.iter().enumerate() {
                for (cell_index, cell) in row.cells.iter().enumerate() {
                    if cell.blocks.contains_node(id) {
                        path.extend([row_index, cell_index]);
                        return find_path(&cell.blocks, id, path);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

enum Relation {
    Outside,
    Inside,
    Boundary,
}

enum Operation<'a> {
    Extract,
    Splice {
        start: &'a [Arc<BlockNode>],
        end: &'a [Arc<BlockNode>],
        next_id: &'a mut u64,
    },
}

struct Walker<'a, 'b> {
    range: &'a TreeRange<'b>,
    operation: Operation<'a>,
}

impl Walker<'_, '_> {
    fn sequence(
        &mut self,
        blocks: &BlockSequence,
        parent: &[usize],
    ) -> Result<BlockSequence, DocumentError> {
        let mut result = Vec::new();
        let mut path = parent.to_vec();
        path.push(0);
        for (index, block) in blocks.iter().enumerate() {
            *path.last_mut().expect("child path has an index") = index;
            match (self.range.relation(&path), &self.operation) {
                (Relation::Outside, Operation::Splice { .. })
                | (Relation::Inside, Operation::Extract) => result.push(block.clone()),
                (Relation::Boundary, _) => result.extend(self.boundary(block, &path)?),
                _ => {}
            }
        }
        Ok(BlockSequence::new(result))
    }

    fn boundary(
        &mut self,
        block: &Arc<BlockNode>,
        path: &[usize],
    ) -> Result<Vec<Arc<BlockNode>>, DocumentError> {
        if let Some(text) = block.text() {
            return match &self.operation {
                Operation::Splice { start, end, .. } => Ok(if path == self.range.start_path {
                    start.to_vec()
                } else {
                    end.to_vec()
                }),
                Operation::Extract => {
                    let from = if path == self.range.start_path {
                        self.range.start.text_offset
                    } else {
                        0
                    };
                    let to = if path == self.range.end_path {
                        self.range.end.text_offset
                    } else {
                        text.len()
                    };
                    let fragment = text.slice(from..to);
                    let mut node = block.as_ref().clone();
                    if matches!(node, BlockNode::Image(_)) && (from != 0 || to != text.len()) {
                        node = BlockNode::Paragraph(Paragraph {
                            id: block.id(),
                            content: fragment,
                        });
                    } else {
                        *node.text_mut().ok_or(PositionError::NotText(block.id()))? = fragment;
                    }
                    Ok(vec![Arc::new(node)])
                }
            };
        }
        let mut node = block.as_ref().clone();
        let keep = match &mut node {
            BlockNode::Definition { blocks, .. }
            | BlockNode::BlockQuote { blocks, .. }
            | BlockNode::Alert { blocks, .. }
            | BlockNode::FootnoteDefinition { blocks, .. } => {
                *blocks = self.sequence(blocks, path)?;
                !blocks.is_empty()
            }
            BlockNode::List(list) => {
                let mut items = Vec::new();
                let mut first_index = None;
                for (index, item) in list.items.iter().enumerate() {
                    let mut item = item.clone();
                    item.blocks = self.sequence(&item.blocks, &child_path(path, index))?;
                    if !item.blocks.is_empty() {
                        first_index.get_or_insert(index);
                        items.push(item);
                    }
                }
                if matches!(self.operation, Operation::Extract)
                    && let ListKind::Ordered { start } = &mut list.kind
                {
                    *start = start.saturating_add(first_index.unwrap_or(0) as u64);
                }
                list.items = items.into();
                if matches!(self.operation, Operation::Splice { .. }) {
                    // Structural replacement can add/remove paragraph gaps.
                    // Match the separators required by canonical Markdown,
                    // while keeping a paragraph plus nested list tight.
                    if matches!(list.items.as_ref(), [item] if item.blocks.len() == 1) {
                        list.tight = true;
                    } else if list.items.iter().any(|item| {
                        item.blocks
                            .iter()
                            .skip(1)
                            .any(|block| !matches!(block.as_ref(), BlockNode::List(_)))
                    }) {
                        list.tight = false;
                    }
                }
                !list.items.is_empty()
            }
            BlockNode::Table(table) => {
                let mut rows = Vec::new();
                let mut first_index = None;
                for (index, row) in table.rows.iter().enumerate() {
                    let row_path = child_path(path, index);
                    if matches!(self.operation, Operation::Extract)
                        && matches!(self.range.relation(&row_path), Relation::Outside)
                    {
                        continue;
                    }
                    first_index.get_or_insert(index);
                    let mut row = row.clone();
                    let mut cells = row.cells.to_vec();
                    for (index, cell) in cells.iter_mut().enumerate() {
                        cell.blocks = self.sequence(&cell.blocks, &child_path(&row_path, index))?;
                        if cell.blocks.is_empty()
                            && let Operation::Splice { next_id, .. } = &mut self.operation
                        {
                            let id = NodeId::new_unchecked(**next_id);
                            **next_id += 1;
                            cell.blocks = BlockSequence::new(vec![Arc::new(BlockNode::Paragraph(
                                Paragraph {
                                    id,
                                    content: RichText::default(),
                                },
                            ))]);
                        }
                    }
                    row.cells = cells.into();
                    rows.push(row);
                }
                if matches!(self.operation, Operation::Extract) {
                    table.header_rows = table
                        .header_rows
                        .saturating_sub(first_index.unwrap_or(0))
                        .min(rows.len());
                }
                table.rows = rows.into();
                !table.rows.is_empty()
            }
            _ => unreachable!("only container nodes can be ancestors of text endpoints"),
        };
        Ok(if keep {
            vec![Arc::new(node)]
        } else {
            Vec::new()
        })
    }
}

fn child_path(parent: &[usize], index: usize) -> Vec<usize> {
    let mut path = parent.to_vec();
    path.push(index);
    path
}
