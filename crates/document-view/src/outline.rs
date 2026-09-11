use document_core::{BlockNode, BlockSequence, NodeId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlineEntry {
    pub node_id: NodeId,
    pub level: u8,
    pub title: String,
}

#[must_use]
pub fn project_outline(blocks: &BlockSequence) -> Vec<OutlineEntry> {
    let mut entries = Vec::new();
    append_outline(blocks, &mut entries);
    entries
}

fn append_outline(blocks: &BlockSequence, output: &mut Vec<OutlineEntry>) {
    for block in blocks {
        match block.as_ref() {
            BlockNode::Heading(heading) => output.push(OutlineEntry {
                node_id: heading.id,
                level: heading.level,
                title: heading.content.as_string(),
            }),
            BlockNode::List(list) => {
                for item in list.items.iter() {
                    append_outline(&item.blocks, output);
                }
            }
            BlockNode::BlockQuote { blocks, .. }
            | BlockNode::Alert { blocks, .. }
            | BlockNode::Definition { blocks, .. }
            | BlockNode::FootnoteDefinition { blocks, .. } => append_outline(blocks, output),
            BlockNode::Table(table) => {
                for row in table.rows.iter() {
                    for cell in row.cells.iter() {
                        append_outline(&cell.blocks, output);
                    }
                }
            }
            _ => {}
        }
    }
}
