use std::{borrow::Cow, sync::Arc};

mod html_source;
mod reference_source;
mod source_reuse;

use comrak::{
    Anchorizer, Arena, Options,
    nodes::{AlertType, AstNode, ListType, NodeValue, Sourcepos, TableAlignment},
    parse_document,
};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::{
    Affinity, AlertKind, BlockNode, BlockSequence, CodeBlock, ColumnAlignment, ColumnSpec,
    DocumentError, DocumentPosition, DocumentSnapshot, Heading, ImageNode, InlineRun, InlineStyle,
    LinkTarget, ListBlock, ListItem, ListKind, NodeId, Paragraph, RichText, Selection, SourceSpine,
    Table, TableCell, TableRow, TextSelection,
    document::{snapshot_from_import, structure_changed},
    source::SourceUnit,
};

pub(crate) fn import(source: Arc<str>) -> Result<DocumentSnapshot, DocumentError> {
    let arena = Arena::new();
    let options = markdown_options();
    let root = parse_document(&arena, &source, &options);
    let mut importer = Importer {
        next_id: 1,
        list_item_positions: Vec::new(),
        note_paragraph_positions: Vec::new(),
        table_cell_positions: Vec::new(),
        leaf_positions: Vec::new(),
        original_source: true,
    };
    let children = root.children().collect::<Vec<_>>();
    let mut blocks = Vec::with_capacity(children.len());
    let mut positions = Vec::with_capacity(children.len());
    let mut index = 0;
    while index < children.len() {
        let child = children[index];
        if let Some(metadata) = recognized_table_metadata(child)
            && let Some(table_node) = children.get(index + 1).copied()
        {
            let value = table_node.data.borrow().value.clone();
            let table = match value {
                NodeValue::Table(table_data)
                    if metadata.widths.len() == table_data.alignments.len() =>
                {
                    Some(importer.table(table_node, &table_data.alignments)?)
                }
                NodeValue::HtmlBlock(html) => crate::html::html_table_data(&html.literal)
                    .filter(|data| metadata.widths.len() == data.alignments.len())
                    .map(|data| importer.html_table(data))
                    .transpose()?,
                _ => None,
            };
            if let Some(mut table) = table {
                metadata.apply(&mut table);
                let block = BlockNode::Table(table);
                let mut source_position = table_node.data.borrow().sourcepos;
                source_position.start = child.data.borrow().sourcepos.start;
                positions.push((block.id(), source_position));
                blocks.push(Arc::new(block));
                index += 2;
                continue;
            }
        }
        if let Some(block) = importer.block(child)? {
            positions.push((block.id(), child.data.borrow().sourcepos));
            blocks.push(Arc::new(block));
        }
        index += 1;
    }
    // Comrak resolves footnotes semantically and may expose their definition
    // nodes after later source blocks. SourceSpine requires source order so
    // every prefix has exactly one owner and unrelated edits cannot duplicate
    // an out-of-order definition from a later block's prefix.
    let mut imported = blocks.into_iter().zip(positions).collect::<Vec<_>>();
    imported.sort_by_key(|(_, (_, position))| {
        (
            position.start.line,
            position.start.column,
            position.end.line,
            position.end.column,
        )
    });
    let (mut blocks, positions): (Vec<_>, Vec<_>) = imported.into_iter().unzip();
    let mut transient_caret = None;
    let first_position = if let Some(position) = first_editable_position(&blocks) {
        position
    } else {
        let id = importer.allocate();
        let caret = Arc::new(BlockNode::Paragraph(Paragraph {
            id,
            content: RichText::default(),
        }));
        blocks.push(caret.clone());
        transient_caret = Some(caret);
        DocumentPosition::new(id, 0, Affinity::Downstream)
    };
    let blocks = BlockSequence::new(blocks);
    let references = reference_source::records(&source, root);
    let spine = build_spine(
        source,
        &positions,
        &blocks,
        &importer.list_item_positions,
        &importer.note_paragraph_positions,
        &importer.table_cell_positions,
        &importer.leaf_positions,
    )
    .with_references(references);
    Ok(snapshot_from_import(
        blocks,
        Selection::Text(TextSelection::caret(first_position)),
        spine,
        importer.next_id,
        transient_caret,
    ))
}

struct ImportedTableMetadata {
    border: crate::TableBorder,
    widths: Vec<Option<f32>>,
    preserved: Vec<String>,
}

impl ImportedTableMetadata {
    fn apply(self, table: &mut Table) {
        table.border = self.border;
        let mut columns = table.columns.to_vec();
        for (column, width) in columns.iter_mut().zip(self.widths) {
            column.width = width;
        }
        table.columns = columns.into();
        table.preserved_metadata = self.preserved.into();
    }
}

fn recognized_table_metadata(node: &AstNode<'_>) -> Option<ImportedTableMetadata> {
    let NodeValue::HtmlBlock(html) = node.data.borrow().value.clone() else {
        return None;
    };
    let literal = html.literal.trim();
    let payload = literal
        .strip_prefix("<!-- tachyon-table:v1 ")?
        .strip_suffix(" -->")?;
    let mut object =
        serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(payload).ok()?;
    let border = serde_json::from_value(object.remove("border")?).ok()?;
    let widths = serde_json::from_value::<Vec<Option<f32>>>(object.remove("widths")?).ok()?;
    if widths
        .iter()
        .flatten()
        .any(|width| !width.is_finite() || *width <= 0.)
    {
        return None;
    }
    let mut preserved = object
        .remove("preserved")
        .map(serde_json::from_value::<Vec<String>>)
        .transpose()
        .ok()?
        .unwrap_or_default();
    preserved.extend(
        object.into_iter().filter_map(|(key, value)| {
            serde_json::to_string(&serde_json::json!({ key: value })).ok()
        }),
    );
    Some(ImportedTableMetadata {
        border,
        widths,
        preserved,
    })
}

fn markdown_options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.tagfilter = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.footnotes = true;
    options.extension.description_lists = true;
    options.extension.alerts = true;
    options.extension.math_dollars = true;
    options.extension.front_matter_delimiter = Some("---".to_owned());
    options.parse.smart = false;
    options.parse.tasklist_in_table = true;
    options
}

#[test]
fn definition_syntax_has_terms_and_descriptions_not_visible_colon_markers() {
    let source =
        "Cache\n: Reusable **prepared geometry**.\n\nViewport\n: The visible document region.\n";
    let document = crate::Document::from_markdown(source).unwrap();
    assert_eq!(document.snapshot().serialize().unwrap(), source);
    assert_eq!(
        document
            .snapshot()
            .blocks()
            .iter()
            .map(|block| block.plain_text())
            .collect::<Vec<_>>()
            .join("\n"),
        "Cache\nReusable prepared geometry.\nViewport\nThe visible document region."
    );
}

#[cfg(test)]
mod definition_tests {
    use super::*;
    use crate::{
        Affinity, DefinitionKind, Document, DocumentPosition, EditCommand, Selection, TextSelection,
    };

    fn leaves(block: &BlockNode, output: &mut Vec<NodeId>) {
        if block.text().is_some() {
            output.push(block.id());
        }
        match block {
            BlockNode::Definition { blocks, .. } | BlockNode::BlockQuote { blocks, .. } => {
                for block in blocks {
                    leaves(block, output);
                }
            }
            BlockNode::List(list) => {
                for item in list.items.iter() {
                    for block in &item.blocks {
                        leaves(block, output);
                    }
                }
            }
            _ => {}
        }
    }

    fn shape(block: &BlockNode) -> String {
        match block {
            BlockNode::Definition { kind, blocks, .. } => format!(
                "{kind:?}({})",
                blocks
                    .iter()
                    .map(|b| shape(b))
                    .collect::<Vec<_>>()
                    .join("|")
            ),
            BlockNode::List(list) => format!(
                "List({})",
                list.items
                    .iter()
                    .map(|i| i
                        .blocks
                        .iter()
                        .map(|b| shape(b))
                        .collect::<Vec<_>>()
                        .join("|"))
                    .collect::<Vec<_>>()
                    .join(";")
            ),
            BlockNode::BlockQuote { blocks, .. } => format!(
                "Quote({})",
                blocks
                    .iter()
                    .map(|b| shape(b))
                    .collect::<Vec<_>>()
                    .join("|")
            ),
            _ => format!(
                "{}:{}",
                match block {
                    BlockNode::Paragraph(_) => "P",
                    BlockNode::CodeBlock(_) => "C",
                    BlockNode::Heading(_) => "H",
                    _ => "other",
                },
                block.plain_text()
            ),
        }
    }

    #[test]
    fn definition_rich_round_trip_and_each_leaf_edit_undo() {
        let source = "# Before\n\nCache\n: Reusable **prepared geometry**.\n: A second description.\n\nViewport\n: A visible region.\n\n    Another paragraph.\n\n    - A child\n    - Another child\n\n    ```rust\n    let x = 1;\n    ```\n\n    Nested\n    : A nested definition.\n\n# After\n\nUntouched *tail*.\n";
        let original = Document::from_markdown(source).unwrap().snapshot();
        let root = original.blocks().get(1).unwrap();
        assert!(matches!(
            root.as_ref(),
            BlockNode::Definition {
                kind: DefinitionKind::List,
                ..
            }
        ));
        let mut ids = Vec::new();
        leaves(root, &mut ids);
        assert!(ids.len() >= 10, "rich leaves: {ids:?}");
        let canonical = serialize_block(root, "\n", 0).unwrap();
        let reopened = Document::from_markdown(canonical.as_str())
            .unwrap()
            .snapshot();
        assert_eq!(
            shape(root),
            shape(reopened.blocks().get(0).unwrap()),
            "{canonical}"
        );
        for id in ids {
            let mut document = Document::from_markdown(source).unwrap();
            document
                .apply(EditCommand::SetSelection(Selection::Text(
                    TextSelection::caret(DocumentPosition::new(id, 0, Affinity::Downstream)),
                )))
                .unwrap();
            let before = document.snapshot();
            document
                .apply(EditCommand::ReplaceSelection {
                    text: "x".into(),
                    typing: false,
                })
                .unwrap();
            let edited = document.snapshot();
            assert!(edited.node(id).unwrap().plain_text().starts_with('x'));
            let serialized = edited.serialize().unwrap();
            assert!(serialized.starts_with("# Before\n\n"));
            assert!(serialized.ends_with("# After\n\nUntouched *tail*.\n"));
            let reopened = Document::from_markdown(serialized.as_str())
                .unwrap()
                .snapshot();
            assert_eq!(
                shape(edited.blocks().get(1).unwrap()),
                shape(reopened.blocks().get(1).unwrap()),
                "{serialized}"
            );
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), source);
            assert_eq!(document.snapshot().selection(), before.selection());
        }
    }

    #[test]
    fn definition_empty_and_split_terms_reopen_as_editable_semantic_html() {
        for (offset, edit) in [
            (0, EditCommand::SplitSelection),
            (2, EditCommand::SplitSelection),
            (4, EditCommand::SplitSelection),
            (
                0,
                EditCommand::ReplaceSelection {
                    text: String::new(),
                    typing: false,
                },
            ),
        ] {
            let source = "Term\n: First description.\n\n    ```rust\n    let x = 1;\n    ```\n";
            let mut document = Document::from_markdown(source).unwrap();
            let snapshot = document.snapshot();
            let mut ids = Vec::new();
            leaves(snapshot.blocks().get(0).unwrap(), &mut ids);
            let mut selection =
                TextSelection::caret(DocumentPosition::new(ids[0], offset, Affinity::Downstream));
            if matches!(edit, EditCommand::ReplaceSelection { .. }) {
                selection.head.text_offset = 4;
            }
            document
                .apply(EditCommand::SetSelection(Selection::Text(selection)))
                .unwrap();
            document.apply(edit).unwrap();
            let edited = document.snapshot();
            let serialized = edited.serialize().unwrap();
            assert!(serialized.contains("<dl>"), "{serialized}");
            let reopened = Document::from_markdown(serialized.as_str())
                .unwrap()
                .snapshot();
            assert_eq!(
                shape(edited.blocks().get(0).unwrap()),
                shape(reopened.blocks().get(0).unwrap()),
                "{serialized}"
            );
            let mut reopened_ids = Vec::new();
            leaves(reopened.blocks().get(0).unwrap(), &mut reopened_ids);
            assert!(reopened_ids.iter().any(|id| matches!(reopened.node(*id), Some(BlockNode::CodeBlock(c)) if c.language.as_deref() == Some("rust"))));
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        }
    }

    #[test]
    fn definition_html_preserves_multiple_terms_empty_groups_and_wrappers() {
        let source = "<dl><dt><strong>CPU</strong></dt><dt>Processor</dt><dd><p>Executes instructions.</p><p></p></dd><dd>Also coordinates work.</dd></dl>\n";
        let document = Document::from_markdown(source).unwrap();
        let snapshot = document.snapshot();
        let root = snapshot.blocks().get(0).unwrap();
        assert!(matches!(
            root.as_ref(),
            BlockNode::Definition {
                kind: DefinitionKind::List,
                ..
            }
        ));
        assert_eq!(snapshot.serialize().unwrap(), source);
        let canonical = serialize_block(root, "\n", 0).unwrap();
        let reopened = Document::from_markdown(canonical.as_str())
            .unwrap()
            .snapshot();
        assert_eq!(
            shape(root),
            shape(reopened.blocks().get(0).unwrap()),
            "{canonical}"
        );
        for source in [
            "<div>Before<dl><dt>Term</dt><dd>Body</dd></dl>After</div>",
            "<dl class=\"custom\"><dt>Term</dt><dd>Body</dd></dl>",
        ] {
            let snapshot = Document::from_markdown(source).unwrap().snapshot();
            assert!(matches!(
                snapshot.blocks().get(0).unwrap().as_ref(),
                BlockNode::PreservedSource { .. }
            ));
            assert_eq!(snapshot.serialize().unwrap(), source);
        }
    }
}

struct Importer {
    next_id: u64,
    list_item_positions: Vec<(NodeId, Sourcepos)>,
    note_paragraph_positions: Vec<(NodeId, Sourcepos)>,
    table_cell_positions: Vec<(NodeId, Sourcepos)>,
    leaf_positions: Vec<(NodeId, Sourcepos)>,
    original_source: bool,
}

impl Importer {
    fn allocate(&mut self) -> NodeId {
        let id = NodeId::new_unchecked(self.next_id);
        self.next_id += 1;
        id
    }

    fn block<'a>(&mut self, node: &'a AstNode<'a>) -> Result<Option<BlockNode>, DocumentError> {
        let value = node.data.borrow().value.clone();
        let source_leaf = (matches!(&value, NodeValue::Paragraph | NodeValue::Heading(_))
            && !node.ancestors().any(|ancestor| {
                matches!(
                    &ancestor.data.borrow().value,
                    NodeValue::DescriptionTerm | NodeValue::DescriptionDetails
                )
            }))
            || matches!(&value, NodeValue::CodeBlock(code) if code.fenced);
        let block = match value {
            NodeValue::Paragraph => {
                let standalone_math = node
                    .first_child()
                    .filter(|child| child.next_sibling().is_none())
                    .and_then(|child| match &child.data.borrow().value {
                        NodeValue::Math(math) if math.display_math => Some(math.literal.clone()),
                        _ => None,
                    });
                if let Some(source) = standalone_math {
                    Some(BlockNode::CodeBlock(CodeBlock {
                        id: self.allocate(),
                        language: Some("math".into()),
                        syntax: crate::CodeBlockSyntax::DisplayMath,
                        content: RichText::new(source),
                    }))
                } else if let Some(image) = standalone_image(node, self.allocate()) {
                    Some(BlockNode::Image(image))
                } else {
                    Some(BlockNode::Paragraph(Paragraph {
                        id: self.allocate(),
                        content: inline_content(node),
                    }))
                }
            }
            NodeValue::Heading(heading) => Some(BlockNode::Heading(Heading {
                id: self.allocate(),
                level: heading.level,
                content: inline_content(node),
            })),
            NodeValue::CodeBlock(code) => {
                let id = self.allocate();
                Some(BlockNode::CodeBlock(CodeBlock {
                    id,
                    language: code.info.split_whitespace().next().map(str::to_owned),
                    syntax: crate::CodeBlockSyntax::Fenced,
                    content: RichText::new(code.literal),
                }))
            }
            NodeValue::BlockQuote | NodeValue::MultilineBlockQuote(_) => {
                Some(BlockNode::BlockQuote {
                    id: self.allocate(),
                    blocks: self.editable_children(node)?,
                })
            }
            NodeValue::List(list) => Some(BlockNode::List(self.list(node, list)?)),
            NodeValue::DescriptionList => {
                let id = self.allocate();
                let blocks = node
                    .children()
                    .flat_map(|item| item.children())
                    .filter_map(|child| self.block(child).transpose())
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(Arc::new)
                    .collect();
                Some(BlockNode::Definition {
                    id,
                    kind: crate::DefinitionKind::List,
                    blocks: BlockSequence::new(blocks),
                })
            }
            NodeValue::DescriptionTerm | NodeValue::DescriptionDetails => {
                Some(BlockNode::Definition {
                    id: self.allocate(),
                    kind: if matches!(value, NodeValue::DescriptionTerm) {
                        crate::DefinitionKind::Term
                    } else {
                        crate::DefinitionKind::Description
                    },
                    blocks: self.children_as_blocks(node)?,
                })
            }
            NodeValue::Table(table) => Some(BlockNode::Table(self.table(node, &table.alignments)?)),
            NodeValue::Alert(alert) => Some(BlockNode::Alert {
                id: self.allocate(),
                kind: match alert.alert_type {
                    AlertType::Note => AlertKind::Note,
                    AlertType::Tip => AlertKind::Tip,
                    AlertType::Important => AlertKind::Important,
                    AlertType::Warning => AlertKind::Warning,
                    AlertType::Caution => AlertKind::Caution,
                },
                title: alert.title.map(RichText::new),
                blocks: self.editable_children(node)?,
            }),
            NodeValue::FootnoteDefinition(definition) => {
                let id = self.allocate();
                let mut blocks = Vec::new();
                for child in node.children() {
                    if let Some(block) = self.block(child)? {
                        // Keep source spans only for direct, original-file note
                        // paragraphs. HTML imports use a different coordinate space.
                        if self.original_source && matches!(block, BlockNode::Paragraph(_)) {
                            self.note_paragraph_positions
                                .push((block.id(), child.data.borrow().sourcepos));
                        }
                        blocks.push(Arc::new(block));
                    }
                }
                Some(BlockNode::FootnoteDefinition {
                    id,
                    label: definition.name,
                    blocks: BlockSequence::new(blocks),
                })
            }
            NodeValue::ThematicBreak => Some(BlockNode::ThematicBreak {
                id: self.allocate(),
            }),
            NodeValue::HtmlBlock(html) => {
                if let Some(table) = crate::html::html_table_data(&html.literal) {
                    Some(BlockNode::Table(self.html_table(table)?))
                } else if let Some(groups) = crate::html::html_definition_data(&html.literal) {
                    let mut definitions = Vec::with_capacity(groups.len());
                    for (kind, fragments) in groups {
                        let id = self.allocate();
                        let mut blocks = Vec::new();
                        for fragment in fragments {
                            if fragment.is_empty() {
                                blocks.push(Arc::new(BlockNode::Paragraph(Paragraph {
                                    id: self.allocate(),
                                    content: RichText::new(""),
                                })));
                            } else {
                                blocks.extend(self.blocks_from_markdown(&fragment)?);
                            }
                        }
                        // An empty label/description remains a real caret host.
                        if blocks.is_empty() {
                            blocks.push(Arc::new(BlockNode::Paragraph(Paragraph {
                                id: self.allocate(),
                                content: RichText::new(""),
                            })));
                        }
                        definitions.push(Arc::new(BlockNode::Definition {
                            id,
                            kind,
                            blocks: BlockSequence::new(blocks),
                        }));
                    }
                    Some(BlockNode::Definition {
                        id: self.allocate(),
                        kind: crate::DefinitionKind::List,
                        blocks: BlockSequence::new(definitions),
                    })
                } else {
                    Some(BlockNode::PreservedSource {
                        id: self.allocate(),
                        description: crate::html::inert_html_fragment(&html.literal)
                            .map(|fragment| fragment.text().trim().to_owned())
                            .filter(|text| !text.is_empty())
                            .unwrap_or_else(|| "Unsupported or preserved HTML".to_owned()),
                        source: Arc::from(html.literal),
                    })
                }
            }
            NodeValue::FrontMatter(front_matter) => Some(BlockNode::PreservedSource {
                id: self.allocate(),
                source: Arc::from(front_matter),
                description: "Front matter".to_owned(),
            }),
            _ => None,
        };
        if self.original_source
            && source_leaf
            && let Some(block) = &block
        {
            self.leaf_positions
                .push((block.id(), node.data.borrow().sourcepos));
        }
        Ok(block)
    }

    fn editable_children<'a>(
        &mut self,
        node: &'a AstNode<'a>,
    ) -> Result<BlockSequence, DocumentError> {
        let blocks = self.children_as_blocks(node)?;
        if blocks.is_empty() {
            Ok(BlockSequence::new(vec![Arc::new(BlockNode::Paragraph(
                Paragraph {
                    id: self.allocate(),
                    content: RichText::default(),
                },
            ))]))
        } else {
            Ok(blocks)
        }
    }

    fn children_as_blocks<'a>(
        &mut self,
        node: &'a AstNode<'a>,
    ) -> Result<BlockSequence, DocumentError> {
        let blocks = node
            .children()
            .filter_map(|child| self.block(child).transpose())
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(Arc::new)
            .collect();
        Ok(BlockSequence::new(blocks))
    }

    fn list<'a>(
        &mut self,
        node: &'a AstNode<'a>,
        list: comrak::nodes::NodeList,
    ) -> Result<ListBlock, DocumentError> {
        let mut items = Vec::new();
        for item_node in node.children() {
            let mut checked = None;
            let mut blocks = Vec::new();
            match item_node.data.borrow().value.clone() {
                NodeValue::Item(_) => {
                    for child in item_node.children() {
                        match child.data.borrow().value.clone() {
                            NodeValue::TaskItem(task) => {
                                checked = Some(task.symbol.is_some());
                                blocks.push(Arc::new(BlockNode::Paragraph(Paragraph {
                                    id: self.allocate(),
                                    content: inline_content(child),
                                })));
                            }
                            _ => {
                                if let Some(block) = self.block(child)? {
                                    blocks.push(Arc::new(block));
                                }
                            }
                        }
                    }
                }
                NodeValue::TaskItem(task) => {
                    checked = Some(task.symbol.is_some());
                    blocks = self.children_as_blocks(item_node)?.to_vec();
                    if blocks.is_empty() {
                        blocks.push(Arc::new(BlockNode::Paragraph(Paragraph {
                            id: self.allocate(),
                            content: inline_content(item_node),
                        })));
                    }
                }
                _ => continue,
            }
            // An empty item, including an unlabeled parent of a nested list,
            // owns a visible marker and a real caret target. Do not steal the
            // child's text as its label or flatten away this hierarchy level.
            // SourceSpine retains the original marker-only spelling unchanged.
            if blocks
                .first()
                .is_none_or(|block| matches!(block.as_ref(), BlockNode::List(_)))
            {
                blocks.insert(
                    0,
                    Arc::new(BlockNode::Paragraph(Paragraph {
                        id: self.allocate(),
                        content: RichText::default(),
                    })),
                );
            }
            let id = self.allocate();
            if self.original_source {
                self.list_item_positions
                    .push((id, item_node.data.borrow().sourcepos));
            }
            items.push(ListItem {
                id,
                checked,
                blocks: BlockSequence::new(blocks),
            });
        }
        let kind = if list.is_task_list {
            ListKind::Task
        } else {
            match list.list_type {
                ListType::Bullet => ListKind::Unordered,
                ListType::Ordered => ListKind::Ordered {
                    start: list.start as u64,
                },
            }
        };
        // Hidden definitions can make the parser report a loose list even
        // when the semantic item contains only one block. There is no
        // inter-item/block gap in that shape, and canonical Markdown
        // cannot persist the distinction after the definition moves away.
        // Keep authored spelling in SourceSpine; normalize only this vacuous
        // spacing flag, not genuine multi-item or multi-block loose lists.
        let tight = list.tight || matches!(items.as_slice(), [item] if item.blocks.len() == 1);
        Ok(ListBlock {
            id: self.allocate(),
            kind,
            tight,
            items: items.into(),
        })
    }

    fn table<'a>(
        &mut self,
        node: &'a AstNode<'a>,
        alignments: &[TableAlignment],
    ) -> Result<Table, DocumentError> {
        let columns: Arc<[ColumnSpec]> = alignments
            .iter()
            .map(|alignment| ColumnSpec {
                alignment: match alignment {
                    TableAlignment::None => ColumnAlignment::None,
                    TableAlignment::Left => ColumnAlignment::Left,
                    TableAlignment::Center => ColumnAlignment::Center,
                    TableAlignment::Right => ColumnAlignment::Right,
                },
                width: None,
            })
            .collect::<Vec<_>>()
            .into();
        let mut rows = Vec::new();
        let mut header_rows = 0;
        for row_node in node.children() {
            let NodeValue::TableRow(header) = row_node.data.borrow().value else {
                continue;
            };
            header_rows += usize::from(header);
            let cells = row_node
                .children()
                .filter(|cell| matches!(cell.data.borrow().value, NodeValue::TableCell))
                .map(|cell| {
                    let id = self.allocate();
                    let paragraph = self.allocate();
                    if self.original_source {
                        self.table_cell_positions
                            .push((paragraph, cell.data.borrow().sourcepos));
                    }
                    TableCell {
                        id,
                        blocks: BlockSequence::new(vec![Arc::new(BlockNode::Paragraph(
                            Paragraph {
                                id: paragraph,
                                content: inline_content(cell),
                            },
                        ))]),
                    }
                })
                .collect::<Vec<_>>()
                .into();
            rows.push(TableRow {
                id: self.allocate(),
                cells,
            });
        }
        Ok(Table {
            id: self.allocate(),
            columns,
            rows: rows.into(),
            header_rows,
            border: crate::TableBorder::default(),
            preserved_metadata: Arc::from([]),
        })
    }

    fn html_table(&mut self, data: crate::html::HtmlTableData) -> Result<Table, DocumentError> {
        let column_count = data.rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
        let columns = (0..column_count)
            .map(|index| ColumnSpec {
                alignment: data.alignments.get(index).copied().unwrap_or_default(),
                width: None,
            })
            .collect::<Vec<_>>()
            .into();
        let mut rows = Vec::with_capacity(data.rows.len());
        for source_row in data.rows {
            let mut cells = Vec::with_capacity(column_count);
            for source_cell in source_row
                .into_iter()
                .chain(std::iter::repeat_with(String::new))
                .take(column_count)
            {
                let mut blocks = self.blocks_from_markdown(&source_cell)?;
                if blocks.is_empty() {
                    blocks.push(Arc::new(BlockNode::Paragraph(Paragraph {
                        id: self.allocate(),
                        content: RichText::default(),
                    })));
                }
                cells.push(TableCell {
                    id: self.allocate(),
                    blocks: BlockSequence::new(blocks),
                });
            }
            rows.push(TableRow {
                id: self.allocate(),
                cells: cells.into(),
            });
        }
        Ok(Table {
            id: self.allocate(),
            columns,
            rows: rows.into(),
            header_rows: data.header_rows,
            border: crate::TableBorder::default(),
            preserved_metadata: Arc::from([]),
        })
    }

    fn blocks_from_markdown(&mut self, source: &str) -> Result<Vec<Arc<BlockNode>>, DocumentError> {
        let arena = Arena::new();
        let root = parse_document(&arena, source, &markdown_options());
        // Converted HTML fragments have their own coordinate space; their
        // offsets must never be interpreted as spans of the original file.
        let original_source = std::mem::replace(&mut self.original_source, false);
        let result = root
            .children()
            .filter_map(|child| self.block(child).transpose())
            .map(|block| block.map(Arc::new))
            .collect();
        self.original_source = original_source;
        result
    }
}

fn inline_content<'a>(node: &'a AstNode<'a>) -> RichText {
    let mut text = String::new();
    let mut runs = Vec::new();
    let mut styles = SmallVec::<[InlineStyle; 3]>::new();
    for child in node.children() {
        append_inline(child, &mut text, &mut runs, &mut styles);
    }
    RichText::from_runs(text, runs)
}

fn append_inline<'a>(
    node: &'a AstNode<'a>,
    text: &mut String,
    runs: &mut Vec<InlineRun>,
    styles: &mut SmallVec<[InlineStyle; 3]>,
) {
    let value = node.data.borrow().value.clone();
    match value {
        NodeValue::Text(value) => push_piece(text, runs, &value, styles),
        NodeValue::Code(code) => {
            styles.push(InlineStyle::Code);
            push_piece(text, runs, &code.literal, styles);
            styles.pop();
        }
        NodeValue::Math(math) => {
            styles.push(InlineStyle::Math {
                display: math.display_math,
            });
            push_piece(text, runs, &math.literal, styles);
            styles.pop();
        }
        NodeValue::SoftBreak => push_piece(text, runs, " ", styles),
        NodeValue::LineBreak => push_piece(text, runs, "  \n", styles),
        NodeValue::Strong => with_style(node, InlineStyle::Bold, text, runs, styles),
        NodeValue::Emph => with_style(node, InlineStyle::Italic, text, runs, styles),
        NodeValue::Strikethrough => {
            with_style(node, InlineStyle::Strikethrough, text, runs, styles);
        }
        NodeValue::Link(link) => with_style(
            node,
            InlineStyle::Link(LinkTarget(link.url)),
            text,
            runs,
            styles,
        ),
        NodeValue::Image(link) => {
            let alt = inline_plain_text(node);
            let style = InlineStyle::Image {
                source: link.url,
                alt: alt.clone(),
                title: (!link.title.is_empty()).then_some(link.title),
            };
            let piece = if alt.is_empty() { "image" } else { &alt };
            let mut image_styles = styles.clone();
            image_styles.push(style);
            push_piece(text, runs, piece, &image_styles);
        }
        NodeValue::FootnoteReference(reference) => {
            let label = format!("[^{}]", reference.name);
            let mut reference_styles = styles.clone();
            reference_styles.push(InlineStyle::FootnoteReference(reference.name));
            push_piece(text, runs, &label, &reference_styles);
        }
        NodeValue::HtmlInline(source) => {
            // GFM table rows cannot contain source newlines. The table writer
            // uses a real HTML break for an authored cell newline; recover
            // that inline meaning rather than displaying the tag as text.
            if matches!(
                source.trim().to_ascii_lowercase().as_str(),
                "<br>" | "<br/>" | "<br />"
            ) && node
                .ancestors()
                .any(|parent| matches!(parent.data.borrow().value, NodeValue::TableCell))
            {
                push_piece(text, runs, "\n", styles);
                return;
            }
            if apply_inline_html_transition(&source, styles) {
                return;
            }
            let mut html_styles = styles.clone();
            html_styles.push(InlineStyle::PreservedHtml(source.clone()));
            push_piece(text, runs, &source, &html_styles);
        }
        _ => {
            for child in node.children() {
                append_inline(child, text, runs, styles);
            }
        }
    }
}

fn apply_inline_html_transition(source: &str, styles: &mut SmallVec<[InlineStyle; 3]>) -> bool {
    let tag = source.trim().to_ascii_lowercase();
    let transition = match tag.as_str() {
        "<strong>" | "<b>" => Some((true, InlineStyle::Bold)),
        "</strong>" | "</b>" => Some((false, InlineStyle::Bold)),
        "<em>" | "<i>" => Some((true, InlineStyle::Italic)),
        "</em>" | "</i>" => Some((false, InlineStyle::Italic)),
        "<del>" | "<s>" | "<strike>" => Some((true, InlineStyle::Strikethrough)),
        "</del>" | "</s>" | "</strike>" => Some((false, InlineStyle::Strikethrough)),
        "<code>" => Some((true, InlineStyle::Code)),
        "</code>" => Some((false, InlineStyle::Code)),
        _ => None,
    };
    let Some((opening, style)) = transition else {
        return false;
    };
    if opening {
        if !styles.contains(&style) {
            styles.push(style);
        }
    } else if let Some(index) = styles.iter().rposition(|candidate| candidate == &style) {
        styles.remove(index);
    }
    true
}

fn with_style<'a>(
    node: &'a AstNode<'a>,
    style: InlineStyle,
    text: &mut String,
    runs: &mut Vec<InlineRun>,
    styles: &mut SmallVec<[InlineStyle; 3]>,
) {
    styles.push(style);
    for child in node.children() {
        append_inline(child, text, runs, styles);
    }
    styles.pop();
}

fn push_piece(
    text: &mut String,
    runs: &mut Vec<InlineRun>,
    piece: &str,
    styles: &SmallVec<[InlineStyle; 3]>,
) {
    let start = text.len();
    text.push_str(piece);
    if start != text.len() {
        runs.push(InlineRun {
            range: start..text.len(),
            styles: styles.clone(),
        });
    }
}

fn inline_plain_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut output = String::new();
    for descendant in node.descendants().skip(1) {
        match &descendant.data.borrow().value {
            NodeValue::Text(text) => output.push_str(text),
            NodeValue::Code(code) => output.push_str(&code.literal),
            NodeValue::SoftBreak | NodeValue::LineBreak => output.push('\n'),
            _ => {}
        }
    }
    output
}

fn standalone_image<'a>(node: &'a AstNode<'a>, id: NodeId) -> Option<ImageNode> {
    let mut children = node.children();
    let child = children.next()?;
    if children.next().is_some() {
        return None;
    }
    let (child, enclosing_link) = if let NodeValue::Link(link) = child.data.borrow().value.clone() {
        let mut linked = child.children();
        let image = linked.next()?;
        if linked.next().is_some() {
            return None;
        }
        (
            image,
            Some(crate::ImageLink {
                target: LinkTarget(link.url),
                title: (!link.title.is_empty()).then_some(link.title),
            }),
        )
    } else {
        (child, None)
    };
    let NodeValue::Image(link) = child.data.borrow().value.clone() else {
        return None;
    };
    Some(ImageNode {
        id,
        source: link.url,
        alt: RichText::new(inline_plain_text(child)),
        title: (!link.title.is_empty()).then_some(link.title),
        intrinsic_size: None,
        link: enclosing_link,
    })
}

fn first_editable_position(blocks: &[Arc<BlockNode>]) -> Option<DocumentPosition> {
    for block in blocks {
        if block.text().is_some() {
            return Some(DocumentPosition::new(block.id(), 0, Affinity::Downstream));
        }
        let nested = match block.as_ref() {
            BlockNode::List(list) => list
                .items
                .iter()
                .find_map(|item| first_editable_position(&item.blocks.to_vec())),
            BlockNode::BlockQuote { blocks, .. }
            | BlockNode::Alert { blocks, .. }
            | BlockNode::Definition { blocks, .. }
            | BlockNode::FootnoteDefinition { blocks, .. } => {
                first_editable_position(&blocks.to_vec())
            }
            BlockNode::Table(table) => table.rows.iter().find_map(|row| {
                row.cells
                    .iter()
                    .find_map(|cell| first_editable_position(&cell.blocks.to_vec()))
            }),
            _ => None,
        };
        if nested.is_some() {
            return nested;
        }
    }
    None
}

fn build_spine(
    source: Arc<str>,
    positions: &[(NodeId, Sourcepos)],
    blocks: &BlockSequence,
    item_positions: &[(NodeId, Sourcepos)],
    note_paragraph_positions: &[(NodeId, Sourcepos)],
    table_cell_positions: &[(NodeId, Sourcepos)],
    leaf_positions: &[(NodeId, Sourcepos)],
) -> SourceSpine {
    let line_starts = line_starts(&source);
    let mut previous_end = 0;
    let mut units = FxHashMap::<NodeId, SourceUnit>::default();
    let mut order = Vec::with_capacity(positions.len());
    let mut owned_roots = FxHashMap::<NodeId, Vec<NodeId>>::default();
    units.reserve(positions.len());
    for (id, position) in positions {
        let Some((start, end)) = source_range(*position, &line_starts, source.len()) else {
            continue;
        };
        if start < previous_end || start > end {
            if start <= end
                && let Some(owner) = order.last()
                && let Some(unit) = units.get_mut(owner)
                && start >= unit.source.start
            {
                // Source-sorted semantic roots may overlap without complete
                // containment. Their connected range has one source owner;
                // extending it prevents the overlapping suffix from becoming
                // an unrelated prefix or a second emitted source fragment.
                unit.source.end = unit.source.end.max(end);
                previous_end = unit.source.end;
                owned_roots.entry(*owner).or_default().push(*id);
            }
            continue;
        }
        units.insert(
            *id,
            SourceUnit {
                prefix: previous_end..start,
                source: start..end,
            },
        );
        order.push(*id);
        previous_end = end;
    }
    let spans = |positions: &[(NodeId, Sourcepos)]| {
        positions
            .iter()
            .filter_map(|(id, position)| {
                let (start, end) = source_range(*position, &line_starts, source.len())?;
                source.get(start..end)?;
                Some((*id, start..end))
            })
            .collect()
    };
    let mut nested = crate::source::NestedSourceSpans {
        list_items: spans(item_positions),
        note_paragraphs: spans(note_paragraph_positions),
        table_cells: spans(table_cell_positions),
        leaf_blocks: spans(leaf_positions),
        html_paragraphs: FxHashMap::default(),
        owned_roots,
        references: FxHashMap::default(),
    };
    for block in blocks {
        if let BlockNode::Table(table) = block.as_ref()
            && let Some(unit) = units.get(&table.id)
            && let Some(mapped) = html_source::paragraph_spans(&source[unit.source.clone()], table)
        {
            nested
                .html_paragraphs
                .extend(mapped.into_iter().map(|(id, range)| {
                    (
                        id,
                        range.start + unit.source.start..range.end + unit.source.start,
                    )
                }));
        }
    }
    SourceSpine::new(source, units, order, previous_end, blocks, nested)
}

fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = Vec::with_capacity(source.len() / 64 + 1);
    starts.push(0);
    starts.extend(source.match_indices('\n').map(|(index, _)| index + 1));
    starts
}

fn source_range(
    position: Sourcepos,
    line_starts: &[usize],
    source_len: usize,
) -> Option<(usize, usize)> {
    let start = line_starts
        .get(position.start.line.checked_sub(1)?)?
        .saturating_add(position.start.column.checked_sub(1)?);
    let end = line_starts
        .get(position.end.line.checked_sub(1)?)?
        .saturating_add(position.end.column)
        .min(source_len);
    (start <= end).then_some((start, end))
}

pub(crate) fn serialize(snapshot: &DocumentSnapshot) -> Result<String, DocumentError> {
    if snapshot.dirty_node_ids().is_empty() && !structure_changed(snapshot) {
        return Ok(snapshot.source_spine().original().to_owned());
    }
    if !structure_changed(snapshot)
        && let Some(source) = source_reuse::serialize_snapshot(snapshot)?
    {
        return Ok(source);
    }

    let newline = snapshot.source_spine().line_ending().as_str();
    let (mut orphaned_before, mut orphaned_tail) = orphaned_prefixes(snapshot);
    let blocks = snapshot
        .blocks()
        .iter()
        .filter(|block| !snapshot.is_transient_caret(block))
        .collect::<Vec<_>>();
    // Lifted footnotes can share their authored bytes with an enclosing root.
    // Preserve that source unit only while its complete semantic group remains
    // adjacent and every edit can be patched inside the unit's proven ranges.
    let positions = blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (b.id(), i))
        .collect::<FxHashMap<_, _>>();
    let mut combined = FxHashMap::default();
    let mut embedded = FxHashSet::default();
    for (owner, children) in snapshot.source_spine().owned_roots() {
        if snapshot.source_was_moved(*owner)
            && !snapshot.source_spine().references(*owner).is_empty()
        {
            continue;
        }
        let Some(index) = positions.get(owner).copied() else {
            continue;
        };
        if children
            .iter()
            .enumerate()
            .all(|(offset, id)| positions.get(id) == Some(&(index + offset + 1)))
            && let Some(source) =
                source_reuse::serialize_owned(&blocks[index..index + children.len() + 1], snapshot)?
        {
            combined.insert(*owner, source);
            embedded.extend(children.iter().copied());
        }
    }
    let blocks = blocks
        .into_iter()
        .filter(|block| !embedded.contains(&block.id()))
        .collect::<Vec<_>>();
    let mut next_source_unit = vec![None; blocks.len()];
    let mut next = None;
    for (index, block) in blocks.iter().enumerate().rev() {
        next_source_unit[index] = next;
        if snapshot.source_spine().unit(block.id()).is_some()
            && !snapshot.source_was_moved(block.id())
        {
            next = Some(block.id());
        }
    }
    let source_order = snapshot
        .source_spine()
        .order()
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect::<FxHashMap<_, _>>();
    let mut output = String::new();
    let mut previous_rewritten = false;
    let mut previous_block = None;
    for (index, block) in blocks.into_iter().enumerate() {
        let dirty = block_or_descendant_dirty(block, snapshot.dirty_node_ids());
        if let Some(unit) = snapshot.source_spine().unit(block.id()) {
            if let Some(prefixes) = orphaned_before.remove(&block.id()) {
                for prefix in prefixes {
                    append_source_record(&mut output, &prefix, newline);
                }
            }
            let prefix = snapshot.source_spine().slice(unit.prefix.clone());
            if !snapshot.source_was_moved(block.id()) || prefix.trim().is_empty() {
                output.push_str(prefix);
            }
            // Canonical serialization does not retain source-owned trailing
            // whitespace (definition lists can own their final blank line).
            // Restore the boundary after any rewritten block without doubling
            // a separator already supplied by the next source-owned prefix.
            // A moved original unit can have an empty prefix (notably the old
            // first block), so changed adjacency requires the same guard.
            let adjacency_changed = previous_block.is_some_and(|previous| {
                source_order
                    .get(&previous)
                    .zip(source_order.get(&block.id()))
                    .is_none_or(|(before, after)| *before + 1 != *after)
            });
            if previous_rewritten || adjacency_changed {
                ensure_blank_line(&mut output, newline);
            }
            if snapshot.source_was_moved(block.id())
                && !snapshot.source_spine().references(block.id()).is_empty()
            {
                // Its declarations are anchored at the original source boundary.
                output.push_str(&serialize_block(block, newline, 0)?);
            } else if let Some(source) = combined.get(&block.id()) {
                output.push_str(source);
            } else if snapshot
                .source_spine()
                .owned_roots()
                .contains_key(&block.id())
            {
                // The source group was split/deleted/restructured. Regenerate
                // the enclosing root without its separately owned definitions.
                output.push_str(&serialize_saved_block(block, snapshot, newline)?);
            } else if dirty {
                let rewritten = source_reuse::serialize(block, snapshot)?;
                output.push_str(&match rewritten {
                    Some(source) => source,
                    None => serialize_saved_block(block, snapshot, newline)?,
                });
            } else {
                output.push_str(snapshot.source_spine().slice(unit.source.clone()));
            }
        } else {
            // A generated block can replace a deleted source-backed block.
            // Emit that block's non-rendered prefix at the replacement
            // boundary instead of delaying it until the next surviving unit.
            // This keeps definitions and other source records on their
            // authored side of converted HTML or a structural replacement.
            let prefixes = next_source_unit[index]
                .and_then(|id| orphaned_before.remove(&id))
                .unwrap_or_else(|| {
                    if next_source_unit[index].is_none() {
                        std::mem::take(&mut orphaned_tail)
                    } else {
                        Vec::new()
                    }
                });
            for prefix in prefixes {
                append_source_record(&mut output, &prefix, newline);
            }
            if index > 0 {
                ensure_blank_line(&mut output, newline);
            }
            output.push_str(&serialize_saved_block(block, snapshot, newline)?);
        }
        previous_rewritten = dirty
            || (snapshot.source_was_moved(block.id())
                && !snapshot.source_spine().references(block.id()).is_empty())
            || snapshot.source_spine().unit(block.id()).is_none()
            || (snapshot
                .source_spine()
                .owned_roots()
                .contains_key(&block.id())
                && !combined.contains_key(&block.id()));
        previous_block = Some(block.id());
    }
    for prefix in orphaned_tail {
        append_source_record(&mut output, &prefix, newline);
    }
    output.push_str(snapshot.source_spine().tail());
    Ok(output)
}

fn append_source_record(output: &mut String, record: &str, newline: &str) {
    if !output.is_empty() && !record.trim().is_empty() {
        let breaks = output
            .chars()
            .rev()
            .take_while(|c| c.is_whitespace())
            .filter(|c| *c == '\n')
            .count()
            + record
                .chars()
                .take_while(|c| c.is_whitespace())
                .filter(|c| *c == '\n')
                .count();
        for _ in breaks..2 {
            output.push_str(newline);
        }
    }
    output.push_str(record);
}

/// Canonical output has no hidden reference declarations. Emit the root's
/// immutable source records in order before its regenerated visible content.
fn serialize_saved_block(
    block: &BlockNode,
    snapshot: &DocumentSnapshot,
    newline: &str,
) -> Result<String, DocumentError> {
    let mut output = String::new();
    for record in snapshot.source_spine().references(block.id()) {
        output.push_str(&record.standalone);
        ensure_blank_line(&mut output, newline);
    }
    output.push_str(&serialize_block(block, newline, 0)?);
    Ok(output)
}

type SourceRecords<'a> = Vec<Cow<'a, str>>;
type OrphanedPrefixes<'a> = (FxHashMap<NodeId, SourceRecords<'a>>, SourceRecords<'a>);

fn orphaned_prefixes(snapshot: &DocumentSnapshot) -> OrphanedPrefixes<'_> {
    let live_ids = snapshot
        .blocks()
        .iter()
        .map(|block| block.id())
        .collect::<FxHashSet<_>>();
    let mut before = FxHashMap::<NodeId, SourceRecords<'_>>::default();
    let mut pending = Vec::new();
    for id in snapshot.source_spine().order() {
        if live_ids.contains(id) && !snapshot.source_was_moved(*id) {
            if !pending.is_empty() {
                before.insert(*id, std::mem::take(&mut pending));
            }
        } else if let Some(unit) = snapshot.source_spine().unit(*id) {
            // Deleted/moved blocks do not take non-rendered declarations with
            // them. Anchor those records before the next unmoved source unit,
            // preserving reference precedence even across repeated moves.
            if !snapshot
                .source_spine()
                .slice(unit.prefix.clone())
                .trim()
                .is_empty()
            {
                pending.push(Cow::Borrowed(
                    snapshot.source_spine().slice(unit.prefix.clone()),
                ));
            }
            let newline = snapshot.source_spine().line_ending().as_str();
            for reference in snapshot.source_spine().references(*id) {
                pending.push(Cow::Owned(format!(
                    "{newline}{newline}{}{newline}{newline}",
                    reference.standalone
                )));
            }
        }
    }
    (before, pending)
}

fn ensure_blank_line(output: &mut String, newline: &str) {
    if !output.ends_with(newline) {
        output.push_str(newline);
    }
    let before_last = &output[..output.len().saturating_sub(newline.len())];
    if !before_last.ends_with(newline) {
        output.push_str(newline);
    }
}

fn block_or_descendant_dirty(
    block: &BlockNode,
    dirty: &std::collections::BTreeSet<NodeId>,
) -> bool {
    if dirty.contains(&block.id()) {
        return true;
    }
    match block {
        BlockNode::List(list) => list.items.iter().any(|item| {
            dirty.contains(&item.id)
                || item
                    .blocks
                    .iter()
                    .any(|block| block_or_descendant_dirty(block, dirty))
        }),
        BlockNode::BlockQuote { blocks, .. }
        | BlockNode::Alert { blocks, .. }
        | BlockNode::Definition { blocks, .. }
        | BlockNode::FootnoteDefinition { blocks, .. } => blocks
            .iter()
            .any(|block| block_or_descendant_dirty(block, dirty)),
        BlockNode::Table(table) => table.rows.iter().any(|row| {
            dirty.contains(&row.id)
                || row.cells.iter().any(|cell| {
                    dirty.contains(&cell.id)
                        || cell
                            .blocks
                            .iter()
                            .any(|block| block_or_descendant_dirty(block, dirty))
                })
        }),
        _ => false,
    }
}

fn serialize_block(
    block: &BlockNode,
    newline: &str,
    indent: usize,
) -> Result<String, DocumentError> {
    Ok(match block {
        // An empty Markdown paragraph is a live editing caret, not a distinct
        // persisted block. Emit no invented markup: surrounding separators
        // keep nonempty paragraphs apart, and import normalizes blank-only
        // topology. Source rebasing defers while those live IDs have no match.
        // Rich HTML containers use their separate explicit paragraph writer.
        BlockNode::Paragraph(paragraph) => serialize_inline(&paragraph.content),
        BlockNode::Heading(heading) => format!(
            "{} {}",
            "#".repeat(usize::from(heading.level.clamp(1, 6))),
            serialize_inline(&heading.content)
        ),
        BlockNode::CodeBlock(code) => {
            let content = code.content.as_string();
            if code.syntax == crate::CodeBlockSyntax::DisplayMath && !content.contains("$$") {
                // Keep the exact formula contents (including authored leading
                // and trailing newlines) inside the original delimiter family.
                return Ok(format!("$${content}$$"));
            }
            let longest_fence = longest_backtick_run(&content).max(2) + 1;
            let fence = "`".repeat(longest_fence);
            let closing_separator = if content.ends_with(['\r', '\n']) {
                ""
            } else {
                newline
            };
            format!(
                "{fence}{}{newline}{}{closing_separator}{fence}",
                code.language.as_deref().unwrap_or(""),
                content
            )
        }
        BlockNode::Image(image) => {
            let figure = format!(
                "![{}]({}{})",
                escape_inline(&image.alt.as_string()),
                serialize_destination(&image.source),
                image
                    .title
                    .as_ref()
                    .map_or_else(String::new, |title| format!(" {}", serialize_title(title)))
            );
            if let Some(link) = &image.link {
                format!(
                    "[{figure}]({}{})",
                    serialize_destination(&link.target.0),
                    link.title
                        .as_ref()
                        .map_or_else(String::new, |title| format!(" {}", serialize_title(title)))
                )
            } else {
                figure
            }
        }
        BlockNode::ThematicBreak { .. } => "---".to_owned(),
        BlockNode::PreservedSource { source, .. } => source.to_string(),
        BlockNode::BlockQuote { blocks, .. } => {
            let inner = serialize_sequence(blocks, newline, indent)?;
            inner
                .split(newline)
                .map(|line| format!("> {line}"))
                .collect::<Vec<_>>()
                .join(newline)
        }
        BlockNode::Alert {
            kind,
            title,
            blocks,
            ..
        } => {
            let kind = match kind {
                AlertKind::Note => "NOTE",
                AlertKind::Tip => "TIP",
                AlertKind::Important => "IMPORTANT",
                AlertKind::Warning => "WARNING",
                AlertKind::Caution => "CAUTION",
                AlertKind::Other(value) => value,
            };
            let title = title
                .as_ref()
                .map_or_else(String::new, |title| format!(" {}", serialize_inline(title)));
            let content = serialize_sequence(blocks, newline, indent)?;
            format!(
                "> [!{kind}]{title}{newline}{}",
                content
                    .split(newline)
                    .map(|line| format!("> {line}"))
                    .collect::<Vec<_>>()
                    .join(newline)
            )
        }
        BlockNode::FootnoteDefinition { label, blocks, .. } => {
            let content = serialize_sequence(blocks, newline, indent + 4)?;
            let mut lines = content.split(newline);
            let first = lines.next().unwrap_or_default();
            let rest = lines
                .map(|line| format!("    {line}"))
                .collect::<Vec<_>>()
                .join(newline);
            if rest.is_empty() {
                format!("[^{label}]: {first}")
            } else {
                format!("[^{label}]: {first}{newline}{rest}")
            }
        }
        BlockNode::List(list) => serialize_list(list, newline, indent)?,
        BlockNode::Definition { kind, blocks, .. } => match kind {
            crate::DefinitionKind::List if !definition_markdown_safe(blocks) => {
                serialize_block_html(block, newline, indent)?
            }
            crate::DefinitionKind::List | crate::DefinitionKind::Term => {
                serialize_sequence(blocks, newline, indent)?
            }
            crate::DefinitionKind::Description => {
                let content = serialize_sequence(blocks, newline, indent)?;
                let mut lines = content.split(newline);
                let mut text = format!(":   {}", lines.next().unwrap_or_default());
                for line in lines {
                    text.push_str(newline);
                    text.push_str("    ");
                    text.push_str(line);
                }
                text
            }
        },
        BlockNode::Table(table) if table.requires_html_serialization() => {
            serialize_html_table(table, newline, indent)?
        }
        BlockNode::Table(table) => serialize_pipe_table(table, newline)?,
    })
}

/// Markdown requires a nonempty, single-paragraph term followed by a
/// nonempty description. Rich edits outside that grammar use semantic HTML
/// rather than flattening paragraphs, losing empty caret hosts or inventing text.
fn definition_markdown_safe(blocks: &BlockSequence) -> bool {
    use crate::DefinitionKind;
    let mut term = false;
    let mut description = false;
    for block in blocks {
        match block.as_ref() {
            BlockNode::Definition {
                kind: DefinitionKind::Term,
                blocks,
                ..
            } => {
                if term && !description {
                    return false;
                }
                if blocks.len() != 1
                    || !matches!(blocks.get(0).map(AsRef::as_ref), Some(BlockNode::Paragraph(p)) if !p.content.as_string().trim().is_empty())
                {
                    return false;
                }
                term = true;
                description = false;
            }
            BlockNode::Definition {
                kind: DefinitionKind::Description,
                blocks,
                ..
            } if term => {
                if blocks.is_empty() || blocks.iter().any(|b| matches!(b.as_ref(), BlockNode::Paragraph(p) if p.content.as_string().trim().is_empty())) { return false; }
                description = true;
            }
            _ => return false,
        }
    }
    term && description
}

fn serialize_sequence(
    blocks: &BlockSequence,
    newline: &str,
    indent: usize,
) -> Result<String, DocumentError> {
    blocks
        .iter()
        .map(|block| serialize_block(block, newline, indent))
        .collect::<Result<Vec<_>, _>>()
        .map(|blocks| blocks.join(&format!("{newline}{newline}")))
}

pub(crate) fn serialize_clipboard_blocks(blocks: &BlockSequence) -> Result<String, DocumentError> {
    serialize_sequence(blocks, "\n", 0)
}

fn serialize_list(list: &ListBlock, newline: &str, indent: usize) -> Result<String, DocumentError> {
    let mut output = String::new();
    for (index, item) in list.items.iter().enumerate() {
        if index > 0 {
            output.push_str(newline);
            if !list.tight {
                output.push_str(newline);
            }
        }
        output.push_str(&" ".repeat(indent));
        let marker_start = output.len();
        match list.kind {
            ListKind::Unordered => output.push_str("- "),
            ListKind::Ordered { start } => output.push_str(&format!("{}. ", start + index as u64)),
            ListKind::Task => output.push_str(match item.checked {
                Some(true) => "- [x] ",
                Some(false) => "- [ ] ",
                None => "- ",
            }),
        }
        let content = serialize_list_content(&item.blocks, newline, list.tight)?;
        // Continuations align with the list marker's content column. Extra
        // spaces become literal content in fenced code. A task checkbox is
        // paragraph content, not part of the Markdown list marker.
        let marker_width = if matches!(list.kind, ListKind::Task) {
            2
        } else {
            output.len() - marker_start
        };
        let continuation = " ".repeat(indent + marker_width);
        output.push_str(&content.replace(newline, &format!("{newline}{continuation}")));
    }
    Ok(output)
}

fn serialize_list_content(
    blocks: &BlockSequence,
    newline: &str,
    tight: bool,
) -> Result<String, DocumentError> {
    let mut output = String::new();
    for (index, block) in blocks.iter().enumerate() {
        if index > 0 {
            output.push_str(newline);
            // A nested list needs no blank separator in a tight item. A
            // blank would change its paragraph grouping when reopened.
            if !tight || !matches!(block.as_ref(), BlockNode::List(_)) {
                output.push_str(newline);
            }
        }
        // The enclosing item owns continuation indentation. Passing its
        // indent down and then prefixing again can turn a nested list to code.
        output.push_str(&serialize_block(block, newline, 0)?);
    }
    Ok(output)
}

fn serialize_pipe_table(table: &Table, newline: &str) -> Result<String, DocumentError> {
    let mut output = table_metadata(table, newline)?;
    for (row_index, row) in table.rows.iter().enumerate() {
        output.push('|');
        for cell in row.cells.iter() {
            let text = cell
                .blocks
                .get(0)
                .and_then(|block| match block.as_ref() {
                    BlockNode::Paragraph(paragraph) => Some(serialize_inline(&paragraph.content)),
                    _ => None,
                })
                .unwrap_or_default()
                .replace('|', "\\|")
                .replace('\n', "<br>");
            output.push(' ');
            output.push_str(&text);
            output.push_str(" |");
        }
        output.push_str(newline);
        if row_index + 1 == table.header_rows.max(1) {
            output.push('|');
            for column in table.columns.iter() {
                let rule = match column.alignment {
                    ColumnAlignment::None => "---",
                    ColumnAlignment::Left => ":--",
                    ColumnAlignment::Center => ":-:",
                    ColumnAlignment::Right => "--:",
                };
                output.push(' ');
                output.push_str(rule);
                output.push_str(" |");
            }
            output.push_str(newline);
        }
    }
    Ok(output.trim_end_matches(['\r', '\n']).to_owned())
}

fn serialize_html_table(
    table: &Table,
    newline: &str,
    indent: usize,
) -> Result<String, DocumentError> {
    serialize_html_table_for(table, newline, indent, HtmlTarget::Clipboard)
}

fn serialize_html_table_for(
    table: &Table,
    newline: &str,
    indent: usize,
    target: HtmlTarget<'_>,
) -> Result<String, DocumentError> {
    let mut output = table_metadata(table, newline)?;
    let authored_width = table
        .columns
        .iter()
        .filter_map(|column| column.width)
        .sum::<f32>();
    if target.is_static() && authored_width > 0. {
        output.push_str(&format!(
            "<table class=\"authored-widths\" style=\"width:{authored_width}px\">"
        ));
        output.push_str(newline);
        output.push_str("<colgroup>");
        for column in table.columns.iter() {
            if let Some(width) = column.width {
                output.push_str(&format!("<col style=\"width:{width}px\">"));
            } else {
                output.push_str("<col>");
            }
        }
        output.push_str("</colgroup>");
    } else {
        output.push_str("<table>");
    }
    output.push_str(newline);
    if target.is_static() {
        output.push_str(if table.header_rows > 0 {
            "<thead>\n"
        } else {
            "<tbody>\n"
        });
    }
    for (row_index, row) in table.rows.iter().enumerate() {
        if target.is_static() && table.header_rows > 0 && row_index == table.header_rows {
            output.push_str("</thead>\n<tbody>\n");
        }
        output.push_str("  <tr>");
        output.push_str(newline);
        for (column_index, cell) in row.cells.iter().enumerate() {
            let tag = if row_index < table.header_rows {
                "th"
            } else {
                "td"
            };
            output.push_str("    <");
            output.push_str(tag);
            let alignment = table
                .columns
                .get(column_index)
                .map_or(ColumnAlignment::None, |column| column.alignment);
            match alignment {
                ColumnAlignment::None => {}
                ColumnAlignment::Left => output.push_str(" align=\"left\""),
                ColumnAlignment::Center => output.push_str(" align=\"center\""),
                ColumnAlignment::Right => output.push_str(" align=\"right\""),
            }
            output.push('>');
            output.push_str(newline);
            output.push_str(&serialize_sequence_html_for(
                &cell.blocks,
                newline,
                indent + 6,
                target,
            )?);
            output.push_str("    </");
            output.push_str(tag);
            output.push('>');
            output.push_str(newline);
        }
        output.push_str("  </tr>");
        output.push_str(newline);
    }
    if target.is_static() {
        output.push_str(
            if table.header_rows > 0 && table.header_rows >= table.rows.len() {
                "</thead>\n"
            } else {
                "</tbody>\n"
            },
        );
    }
    output.push_str("</table>");
    Ok(output)
}

fn table_metadata(table: &Table, newline: &str) -> Result<String, DocumentError> {
    if table.border == crate::TableBorder::default()
        && table.columns.iter().all(|column| column.width.is_none())
        && table.preserved_metadata.is_empty()
    {
        return Ok(String::new());
    }
    let mut metadata = serde_json::Map::new();
    metadata.insert(
        "border".into(),
        serde_json::to_value(table.border)
            .map_err(|error| DocumentError::Markdown(error.to_string()))?,
    );
    metadata.insert(
        "widths".into(),
        serde_json::to_value(
            table
                .columns
                .iter()
                .map(|column| column.width)
                .collect::<Vec<_>>(),
        )
        .map_err(|error| DocumentError::Markdown(error.to_string()))?,
    );
    let mut opaque = Vec::new();
    for preserved in table.preserved_metadata.iter() {
        if let Ok(fields) =
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(preserved)
        {
            for (key, value) in fields {
                if !matches!(key.as_str(), "border" | "widths" | "preserved") {
                    metadata.entry(key).or_insert(value);
                }
            }
        } else {
            opaque.push(preserved);
        }
    }
    if !opaque.is_empty() {
        metadata.insert(
            "preserved".into(),
            serde_json::to_value(opaque)
                .map_err(|error| DocumentError::Markdown(error.to_string()))?,
        );
    }
    let metadata = serde_json::to_string(&metadata)
        .map_err(|error| DocumentError::Markdown(error.to_string()))?;
    let metadata = metadata.replace("--", "\\u002d\\u002d");
    Ok(format!("<!-- tachyon-table:v1 {metadata} -->{newline}"))
}

pub(crate) fn serialize_inline(text: &RichText) -> String {
    if inline_styles_need_html_boundaries(text) {
        return preserve_inline_edge_spaces(serialize_inline_with_html_boundaries(text));
    }
    let source = text.as_string();
    let mut output = String::new();
    for run in text.runs() {
        let value = &source[run.range.clone()];
        if let Some(InlineStyle::FootnoteReference(label)) = run
            .styles
            .iter()
            .find(|style| matches!(style, InlineStyle::FootnoteReference(_)))
        {
            output.push_str(&format!("[^{label}]"));
            continue;
        }
        if let Some(InlineStyle::PreservedHtml(html)) = run
            .styles
            .iter()
            .find(|style| matches!(style, InlineStyle::PreservedHtml(_)))
        {
            output.push_str(html);
            continue;
        }

        let formatting: Vec<_> = run
            .styles
            .iter()
            .filter(|style| {
                matches!(
                    style,
                    InlineStyle::Bold
                        | InlineStyle::Italic
                        | InlineStyle::Strikethrough
                        | InlineStyle::Link(_)
                )
            })
            .collect();
        for style in &formatting {
            output.push_str(open_style(style));
        }
        if let Some(image) = serialize_inline_image(&run.styles) {
            output.push_str(&image);
        } else if let Some(InlineStyle::Math { display }) = run
            .styles
            .iter()
            .find(|style| matches!(style, InlineStyle::Math { .. }))
        {
            let delimiter = if *display { "$$" } else { "$" };
            output.push_str(&format!("{delimiter}{value}{delimiter}"));
        } else if run.styles.contains(&InlineStyle::Code) {
            output.push_str(&serialize_code_span(value));
        } else {
            output.push_str(&escape_inline(value));
        }
        for style in formatting.iter().rev() {
            output.push_str(&close_style(style));
        }
    }
    preserve_inline_edge_spaces(output)
}

// A split or partial paste can leave significant spaces at a paragraph's edge.
// Literal Markdown whitespace there is indentation or gets trimmed on import;
// entities preserve the canonical text without changing its copied characters.
fn preserve_inline_edge_spaces(value: String) -> String {
    let first = value.len() - value.trim_start_matches([' ', '\t']).len();
    let last = value.trim_end_matches([' ', '\t']).len().max(first);
    if first == 0 && last == value.len() {
        return value;
    }
    let mut output = String::with_capacity(value.len());
    for (index, character) in value.char_indices() {
        match character {
            ' ' if index < first || index >= last => output.push_str("&#32;"),
            '\t' if index < first || index >= last => output.push_str("&#9;"),
            _ => output.push(character),
        }
    }
    output
}

fn serialize_inline_image(styles: &[InlineStyle]) -> Option<String> {
    let (source, alt, title) = styles.iter().find_map(|style| match style {
        InlineStyle::Image { source, alt, title } => Some((source, alt, title)),
        _ => None,
    })?;
    Some(format!(
        "![{}]({}{})",
        escape_inline(alt),
        serialize_destination(source),
        title
            .as_ref()
            .map_or_else(String::new, |title| format!(" {}", serialize_title(title)))
    ))
}

fn inline_styles_need_html_boundaries(text: &RichText) -> bool {
    if inline_styles_cross(text) {
        return true;
    }

    let source = text.as_string();
    text.runs().iter().any(|run| {
        let uses_markdown_delimiter = run.styles.iter().any(|style| {
            matches!(
                style,
                InlineStyle::Bold | InlineStyle::Italic | InlineStyle::Strikethrough
            )
        });
        uses_markdown_delimiter
            && source.get(run.range.clone()).is_some_and(|value| {
                value.chars().next().is_some_and(char::is_whitespace)
                    || value.chars().next_back().is_some_and(char::is_whitespace)
            })
    })
}

fn inline_styles_cross(text: &RichText) -> bool {
    let mut intervals = Vec::<(usize, usize, InlineStyle)>::new();
    for run in text.runs() {
        for style in run.styles.iter().filter(|style| {
            matches!(
                style,
                InlineStyle::Bold
                    | InlineStyle::Italic
                    | InlineStyle::Strikethrough
                    | InlineStyle::Link(_)
            )
        }) {
            if let Some(previous) = intervals
                .iter_mut()
                .rev()
                .find(|(_, _, candidate)| candidate == style)
                && previous.1 == run.range.start
            {
                previous.1 = run.range.end;
            } else {
                intervals.push((run.range.start, run.range.end, style.clone()));
            }
        }
    }
    intervals.iter().enumerate().any(|(left_index, left)| {
        intervals.iter().skip(left_index + 1).any(|right| {
            (left.0 < right.0 && right.0 < left.1 && left.1 < right.1)
                || (right.0 < left.0 && left.0 < right.1 && right.1 < left.1)
        })
    })
}

fn serialize_inline_with_html_boundaries(text: &RichText) -> String {
    let source = text.as_string();
    let mut output = String::new();
    for run in text.runs() {
        let value = &source[run.range.clone()];
        if let Some(InlineStyle::FootnoteReference(label)) = run
            .styles
            .iter()
            .find(|style| matches!(style, InlineStyle::FootnoteReference(_)))
        {
            output.push_str(&format!("[^{label}]"));
            continue;
        }
        if let Some(InlineStyle::PreservedHtml(html)) = run
            .styles
            .iter()
            .find(|style| matches!(style, InlineStyle::PreservedHtml(_)))
        {
            output.push_str(html);
            continue;
        }

        for style in &run.styles {
            output.push_str(match style {
                InlineStyle::Bold => "<strong>",
                InlineStyle::Italic => "<em>",
                InlineStyle::Strikethrough => "<del>",
                InlineStyle::Link(_) => "[",
                _ => "",
            });
        }
        if let Some(image) = serialize_inline_image(&run.styles) {
            output.push_str(&image);
        } else if let Some(InlineStyle::Math { display }) = run
            .styles
            .iter()
            .find(|style| matches!(style, InlineStyle::Math { .. }))
        {
            let delimiter = if *display { "$$" } else { "$" };
            output.push_str(&format!("{delimiter}{value}{delimiter}"));
        } else if run.styles.contains(&InlineStyle::Code) {
            output.push_str(&serialize_code_span(value));
        } else {
            output.push_str(&escape_inline(value));
        }
        for style in run.styles.iter().rev() {
            match style {
                InlineStyle::Bold => output.push_str("</strong>"),
                InlineStyle::Italic => output.push_str("</em>"),
                InlineStyle::Strikethrough => output.push_str("</del>"),
                InlineStyle::Link(LinkTarget(target)) => {
                    output.push_str(&format!("]({})", serialize_destination(target)));
                }
                _ => {}
            }
        }
    }
    output
}

pub(crate) fn clipboard_html(markdown: &str) -> String {
    comrak::markdown_to_html(markdown, &markdown_options())
}

fn open_style(style: &InlineStyle) -> &str {
    match style {
        InlineStyle::Bold => "**",
        InlineStyle::Italic => "*",
        InlineStyle::Strikethrough => "~~",
        InlineStyle::Link(_) => "[",
        _ => "",
    }
}

fn close_style(style: &InlineStyle) -> String {
    match style {
        InlineStyle::Bold => "**".to_owned(),
        InlineStyle::Italic => "*".to_owned(),
        InlineStyle::Strikethrough => "~~".to_owned(),
        InlineStyle::Link(LinkTarget(target)) => format!("]({})", serialize_destination(target)),
        _ => String::new(),
    }
}

pub(crate) fn escape_inline(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        // CommonMark permits every ASCII punctuation character to be
        // backslash-escaped. Escaping plain runs broadly prevents text that
        // begins with a block marker, resembles an entity, or looks like an
        // autolink from acquiring semantics after an edit. Pipe is handled by
        // the table serializer so it is not escaped twice there.
        if character.is_ascii_punctuation() && character != '|' {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

pub(crate) fn serialize_code_span(value: &str) -> String {
    if value.contains(['\r', '\n']) {
        let value = escape_html(value)
            .replace('\r', "&#13;")
            .replace('\n', "&#10;");
        return format!("<code>{value}</code>");
    }
    let fence = "`".repeat(longest_backtick_run(value).saturating_add(1).max(1));
    let needs_padding = value.starts_with('`')
        || value.ends_with('`')
        || (value.starts_with(' ')
            && value.ends_with(' ')
            && !value.chars().all(|character| character == ' '));
    if needs_padding {
        format!("{fence} {value} {fence}")
    } else {
        format!("{fence}{value}{fence}")
    }
}

pub(crate) fn serialize_destination(target: &str) -> String {
    if !target.chars().any(|character| {
        character.is_ascii_whitespace()
            || matches!(character, '&' | '\\' | '(' | ')' | '<' | '>' | '"' | '\'')
    }) {
        return target.to_owned();
    }
    // Keep complex targets in CommonMark's bare form. Entity-encode syntax
    // delimiters and double literal backslashes: the angle form consumes a
    // backslash before punctuation and cannot preserve that target exactly.
    let mut escaped = String::with_capacity(target.len());
    for character in target.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '\\' => escaped.push_str("\\\\"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            '(' => escaped.push_str("&#40;"),
            ')' => escaped.push_str("&#41;"),
            '\n' => escaped.push_str("&#10;"),
            '\r' => escaped.push_str("&#13;"),
            '\t' => escaped.push_str("&#9;"),
            ' ' => escaped.push_str("&#32;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

pub(crate) fn serialize_title(title: &str) -> String {
    let mut escaped = String::with_capacity(title.len() + 2);
    escaped.push('"');
    for character in title.chars() {
        match character {
            '\\' | '"' => {
                escaped.push('\\');
                escaped.push(character);
            }
            '\n' => escaped.push_str("&#10;"),
            '\r' => escaped.push_str("&#13;"),
            '&' => escaped.push_str("&amp;"),
            _ => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

/// Mixed preview copy uses canonical roots before applying export sanitization.
pub(crate) fn clipboard_html_blocks(blocks: &BlockSequence) -> Result<String, DocumentError> {
    let html = serialize_sequence_html(blocks, "\n", 0)?;
    crate::html::clipboard_html_document(&html).ok_or_else(|| {
        DocumentError::Clipboard("Selection cannot be safely exported as HTML".into())
    })
}

pub(crate) fn static_html_blocks(blocks: &BlockSequence) -> Result<String, DocumentError> {
    let mut heading_ids = FxHashMap::default();
    collect_heading_ids(blocks, &mut Anchorizer::new(), &mut heading_ids);
    serialize_sequence_html_for(blocks, "\n", 0, HtmlTarget::Static(&heading_ids))
}

#[derive(Clone, Copy)]
enum HtmlTarget<'a> {
    Clipboard,
    Static(&'a FxHashMap<NodeId, String>),
}

impl<'a> HtmlTarget<'a> {
    fn is_static(self) -> bool {
        matches!(self, Self::Static(_))
    }

    fn heading_id(self, node: NodeId) -> Option<&'a str> {
        match self {
            Self::Clipboard => None,
            Self::Static(ids) => ids.get(&node).map(String::as_str),
        }
    }
}

fn collect_heading_ids(
    blocks: &BlockSequence,
    anchorizer: &mut Anchorizer,
    output: &mut FxHashMap<NodeId, String>,
) {
    for block in blocks {
        match block.as_ref() {
            BlockNode::Heading(heading) => {
                output.insert(
                    heading.id,
                    anchorizer.anchorize(&heading.content.as_string()),
                );
            }
            BlockNode::List(list) => {
                for item in list.items.iter() {
                    collect_heading_ids(&item.blocks, anchorizer, output);
                }
            }
            BlockNode::BlockQuote { blocks, .. }
            | BlockNode::Alert { blocks, .. }
            | BlockNode::Definition { blocks, .. }
            | BlockNode::FootnoteDefinition { blocks, .. } => {
                collect_heading_ids(blocks, anchorizer, output);
            }
            BlockNode::Table(table) => {
                for row in table.rows.iter() {
                    for cell in row.cells.iter() {
                        collect_heading_ids(&cell.blocks, anchorizer, output);
                    }
                }
            }
            _ => {}
        }
    }
}

fn serialize_sequence_html(
    blocks: &BlockSequence,
    newline: &str,
    indent: usize,
) -> Result<String, DocumentError> {
    serialize_sequence_html_for(blocks, newline, indent, HtmlTarget::Clipboard)
}

fn serialize_sequence_html_for(
    blocks: &BlockSequence,
    newline: &str,
    indent: usize,
    target: HtmlTarget<'_>,
) -> Result<String, DocumentError> {
    let mut output = String::new();
    for block in blocks {
        output.push_str(&" ".repeat(indent));
        output.push_str(&serialize_block_html_for(block, newline, indent, target)?);
        output.push_str(newline);
    }
    Ok(output)
}

fn serialize_block_html(
    block: &BlockNode,
    newline: &str,
    indent: usize,
) -> Result<String, DocumentError> {
    serialize_block_html_for(block, newline, indent, HtmlTarget::Clipboard)
}

fn serialize_block_html_for(
    block: &BlockNode,
    newline: &str,
    indent: usize,
    target: HtmlTarget<'_>,
) -> Result<String, DocumentError> {
    Ok(match block {
        BlockNode::Paragraph(paragraph) => {
            format!(
                "<p>{}</p>",
                serialize_inline_html_for(&paragraph.content, target)
            )
        }
        BlockNode::Heading(heading) => {
            let id = target
                .heading_id(heading.id)
                .map_or_else(String::new, |id| format!(" id=\"{}\"", escape_html(id)));
            format!(
                "<h{level}{id}>{}</h{level}>",
                serialize_inline_html_for(&heading.content, target),
                level = heading.level.clamp(1, 6)
            )
        }
        BlockNode::CodeBlock(code) => {
            let class = code.language.as_ref().map_or_else(String::new, |language| {
                format!(" class=\"language-{}\"", escape_html(language))
            });
            let language = if target.is_static() {
                code.language.as_ref().map_or_else(String::new, |language| {
                    format!(" data-language=\"{}\"", escape_html(language))
                })
            } else {
                String::new()
            };
            format!(
                "<pre{language}><code{class}>{}</code></pre>",
                escape_html_literal_text(&code.content.as_string())
            )
        }
        BlockNode::Image(image) => {
            let figure = if target.is_static() && !safe_static_url(&image.source) {
                escape_html(&image.alt.as_string())
            } else {
                format!(
                    "<img src=\"{}\" alt=\"{}\"{}>",
                    escape_html(&image.source),
                    escape_html(&image.alt.as_string()),
                    image.title.as_ref().map_or_else(String::new, |title| {
                        format!(" title=\"{}\"", escape_html(title))
                    })
                )
            };
            if let Some(link) = &image.link {
                if target.is_static() && !safe_static_url(&link.target.0) {
                    format!("<p>{figure}</p>")
                } else {
                    format!(
                        "<p><a href=\"{}\"{}>{figure}</a></p>",
                        escape_html(&link.target.0),
                        link.title.as_ref().map_or_else(String::new, |title| {
                            format!(" title=\"{}\"", escape_html(title))
                        })
                    )
                }
            } else {
                format!("<p>{figure}</p>")
            }
        }
        BlockNode::BlockQuote { blocks, .. } => format!(
            "<blockquote>{newline}{}</blockquote>",
            serialize_sequence_html_for(blocks, newline, indent + 2, target)?
        ),
        BlockNode::List(list) => {
            let is_task = matches!(list.kind, ListKind::Task);
            let (tag, start) = match list.kind {
                ListKind::Ordered { start } => ("ol", format!(" start=\"{start}\"")),
                ListKind::Unordered | ListKind::Task => ("ul", String::new()),
            };
            let class = if target.is_static() && is_task {
                " class=\"task-list\""
            } else {
                ""
            };
            let mut list_html = format!("<{tag}{start}{class}>{newline}");
            for item in list.items.iter() {
                list_html.push_str(&" ".repeat(indent + 2));
                list_html.push_str(if target.is_static() && is_task {
                    "<li class=\"task-item\">"
                } else {
                    "<li>"
                });
                if let Some(checked) = item.checked {
                    list_html.push_str(if checked {
                        "<input type=\"checkbox\" checked disabled>"
                    } else {
                        "<input type=\"checkbox\" disabled>"
                    });
                }
                list_html.push_str(newline);
                list_html.push_str(&serialize_sequence_html_for(
                    &item.blocks,
                    newline,
                    indent + 4,
                    target,
                )?);
                list_html.push_str(&" ".repeat(indent + 2));
                list_html.push_str("</li>");
                list_html.push_str(newline);
            }
            list_html.push_str(&" ".repeat(indent));
            list_html.push_str(&format!("</{tag}>"));
            list_html
        }
        BlockNode::Definition { kind, blocks, .. } => {
            let tag = match kind {
                crate::DefinitionKind::List => "dl",
                crate::DefinitionKind::Term => "dt",
                crate::DefinitionKind::Description => "dd",
            };
            format!(
                "<{tag}>{newline}{}</{tag}>",
                serialize_sequence_html_for(blocks, newline, indent + 2, target)?
            )
        }
        BlockNode::Alert {
            kind,
            title,
            blocks,
            ..
        } => {
            let kind = match kind {
                crate::AlertKind::Note => "note",
                crate::AlertKind::Tip => "tip",
                crate::AlertKind::Important => "important",
                crate::AlertKind::Warning => "warning",
                crate::AlertKind::Caution => "caution",
                crate::AlertKind::Other(value) => value,
            };
            let title = title.as_ref().map_or_else(String::new, |title| {
                format!(
                    "<strong>{}</strong>{newline}",
                    serialize_inline_html_for(title, target)
                )
            });
            format!(
                "<aside data-alert=\"{}\">{newline}{}{}</aside>",
                escape_html(kind),
                title,
                serialize_sequence_html_for(blocks, newline, indent + 2, target)?
            )
        }
        BlockNode::FootnoteDefinition { label, blocks, .. } => format!(
            "<section role=\"doc-footnote\" id=\"fn-{}\">{newline}{}</section>",
            escape_html(label),
            serialize_sequence_html_for(blocks, newline, indent + 2, target)?
        ),
        BlockNode::ThematicBreak { .. } => "<hr>".into(),
        // Serialization is source preservation, not rendering. An untouched
        // HTML fragment in a changed table must survive save/reopen with its
        // authored attributes and disclosure state. The renderer still applies
        // its independent inert/sanitized policy when displaying this source.
        BlockNode::PreservedSource { source, .. } if target.is_static() => {
            crate::html::clipboard_html_fragment(source).unwrap_or_else(|| {
                format!(
                    "<pre class=\"source-fallback\"><code>{}</code></pre>",
                    escape_html_literal_text(source)
                )
            })
        }
        BlockNode::PreservedSource { source, .. } => source.to_string(),
        BlockNode::Table(table) => serialize_html_table_for(table, newline, indent, target)?,
    })
}

fn serialize_inline_html(text: &RichText) -> String {
    serialize_inline_html_for(text, HtmlTarget::Clipboard)
}

fn serialize_inline_html_for(text: &RichText, target: HtmlTarget<'_>) -> String {
    let source = text.as_string();
    let mut output = String::new();
    for run in text.runs() {
        let value = &source[run.range.clone()];
        if let Some(InlineStyle::FootnoteReference(label)) = run
            .styles
            .iter()
            .find(|style| matches!(style, InlineStyle::FootnoteReference(_)))
        {
            output.push_str(&format!(
                "<sup><a href=\"#fn-{}\">{}</a></sup>",
                escape_html(label),
                escape_html(value)
            ));
            continue;
        }
        let formatting = run
            .styles
            .iter()
            .filter(|style| {
                matches!(
                    style,
                    InlineStyle::Bold
                        | InlineStyle::Italic
                        | InlineStyle::Strikethrough
                        | InlineStyle::Code
                        | InlineStyle::Link(_)
                )
            })
            .collect::<Vec<_>>();
        for style in &formatting {
            match style {
                InlineStyle::Bold => output.push_str("<strong>"),
                InlineStyle::Italic => output.push_str("<em>"),
                InlineStyle::Strikethrough => output.push_str("<del>"),
                InlineStyle::Code => output.push_str("<code>"),
                InlineStyle::Link(LinkTarget(link))
                    if !target.is_static() || safe_static_url(link) =>
                {
                    output.push_str(&format!("<a href=\"{}\">", escape_html(link)));
                }
                _ => {}
            }
        }
        if let Some(InlineStyle::Image { source, alt, title }) = run
            .styles
            .iter()
            .find(|style| matches!(style, InlineStyle::Image { .. }))
        {
            if target.is_static() && !safe_static_url(source) {
                output.push_str(&escape_html(alt));
            } else {
                output.push_str(&format!(
                    "<img src=\"{}\" alt=\"{}\"{}>",
                    escape_html(source),
                    escape_html(alt),
                    title.as_ref().map_or_else(String::new, |title| format!(
                        " title=\"{}\"",
                        escape_html(title)
                    ))
                ));
            }
        } else if formatting
            .iter()
            .any(|style| matches!(style, InlineStyle::Code))
        {
            // Raw blank lines terminate a Markdown HTML block, and HTML
            // parsing normalizes CRLF. Entities retain the exact code text.
            output.push_str(&escape_html_literal_text(value));
        } else {
            output.push_str(&escape_html_inline_text(value));
        }
        for style in formatting.iter().rev() {
            output.push_str(match style {
                InlineStyle::Bold => "</strong>",
                InlineStyle::Italic => "</em>",
                InlineStyle::Strikethrough => "</del>",
                InlineStyle::Code => "</code>",
                InlineStyle::Link(LinkTarget(link))
                    if !target.is_static() || safe_static_url(link) =>
                {
                    "</a>"
                }
                InlineStyle::Link(_) => "",
                _ => "",
            });
        }
    }
    output
}

fn longest_backtick_run(value: &str) -> usize {
    value
        .split(|character| character != '`')
        .map(str::len)
        .max()
        .unwrap_or(0)
}

fn safe_static_url(value: &str) -> bool {
    if value.is_empty()
        || value.trim() != value
        || value.chars().any(char::is_control)
        || value.starts_with("//")
    {
        return false;
    }
    let scheme_end = value
        .find(':')
        .filter(|colon| !value[..*colon].contains(['/', '?', '#']));
    let Some(scheme_end) = scheme_end else {
        return true;
    };
    matches!(
        value[..scheme_end].to_ascii_lowercase().as_str(),
        "http" | "https" | "mailto"
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn escape_html_literal_text(value: &str) -> String {
    escape_html(value)
        .replace('\r', "&#13;")
        .replace('\n', "&#10;")
}

fn escape_html_inline_text(value: &str) -> String {
    let escaped = escape_html_literal_text(value);
    if value.contains(['\r', '\n', '\t'])
        || value.starts_with(' ')
        || value.ends_with(' ')
        || value.contains("  ")
    {
        // CSS gives significant editing whitespace its literal display
        // semantics. Import recognizes this text-only span and avoids the
        // intermediate Markdown parser's whitespace normalization.
        format!("<span style=\"white-space: pre-wrap\">{escaped}</span>")
    } else {
        escaped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_root_ranges_form_one_complete_source_unit() {
        // Exercise the source-position contract directly: a partial overlap,
        // then a contained root, followed by a separate source unit.
        let source: Arc<str> = Arc::from("alpha\nbeta\ngamma\nomega\n");
        let ids = [1, 2, 3, 4].map(NodeId::new_unchecked);
        let blocks = BlockSequence::new(
            ids.iter()
                .map(|id| {
                    Arc::new(BlockNode::Paragraph(Paragraph {
                        id: *id,
                        content: RichText::new("text"),
                    }))
                })
                .collect(),
        );
        let positions = [
            (ids[0], (1, 1, 2, 4).into()),
            (ids[1], (2, 1, 3, 5).into()),
            (ids[2], (3, 1, 3, 5).into()),
            (ids[3], (4, 1, 4, 5).into()),
        ];
        let spine = build_spine(source.clone(), &positions, &blocks, &[], &[], &[], &[]);
        assert_eq!(spine.unit(ids[0]).unwrap().source, 0..16);
        assert_eq!(spine.order(), &[ids[0], ids[3]]);
        assert_eq!(
            spine.owned_roots().get(&ids[0]),
            Some(&vec![ids[1], ids[2]])
        );
        assert_eq!(spine.unit(ids[3]).unwrap().prefix, 16..17);
        let mut restored = String::new();
        for id in spine.order() {
            let unit = spine.unit(*id).unwrap();
            restored.push_str(spine.slice(unit.prefix.clone()));
            restored.push_str(spine.slice(unit.source.clone()));
        }
        restored.push_str(spine.tail());
        assert_eq!(restored, source.as_ref());
    }

    #[test]
    fn nested_canonical_table_export_keeps_every_cell() {
        let document = crate::Document::from_markdown("| Outer |\n| --- |\n| Container |\n\n| Nested key | Nested value |\n| --- | ---: |\n| Retry count | 17 |\n").unwrap();
        let snapshot = document.snapshot();
        let BlockNode::Table(outer) = snapshot.blocks().get(0).unwrap().as_ref() else {
            panic!("outer table")
        };
        let nested = snapshot.blocks().get(1).unwrap().clone();
        let mut outer = outer.clone();
        let mut rows = outer.rows.to_vec();
        let mut cells = rows[1].cells.to_vec();
        cells[0].blocks = BlockSequence::new(vec![nested]);
        rows[1].cells = cells.into();
        outer.rows = rows.into();
        let html = serialize_html_table(&outer, "\n", 0).unwrap();
        assert_eq!(html.matches("<table>").count(), 2, "{html}");
        for content in [
            "Nested key",
            "Nested value",
            "Retry count",
            "17",
            "align=\"right\"",
        ] {
            assert!(html.contains(content), "missing {content}: {html}");
        }
        assert!(!html.contains("data-unsupported"));
        let reopened = crate::Document::from_markdown(html.clone()).unwrap();
        assert_eq!(reopened.snapshot().serialize().unwrap(), html);
    }

    #[test]
    fn inline_images_keep_enclosing_links_and_formatting_in_every_export_path() {
        let doc = crate::Document::from_markdown(
            "**[![First](a.png \"Title\")](guide.md)** ![Second](b.png)",
        )
        .unwrap();
        let snapshot = doc.snapshot();
        let text = snapshot.blocks().iter().next().unwrap().text().unwrap();
        assert!(
            text.runs()[0]
                .styles
                .contains(&InlineStyle::Link(LinkTarget("guide.md".into()))),
            "import retains the enclosing link"
        );
        for output in [
            serialize_inline(text),
            serialize_inline_with_html_boundaries(text),
        ] {
            assert!(
                output.contains("[![First](a.png \"Title\")](guide.md)"),
                "{output}"
            );
            let reparsed = crate::Document::from_markdown(output).unwrap();
            let result = reparsed.snapshot();
            let text = result.blocks().iter().next().unwrap().text().unwrap();
            assert!(text.runs()[0].styles.contains(&InlineStyle::Bold));
            assert!(
                text.runs()[0]
                    .styles
                    .contains(&InlineStyle::Link(LinkTarget("guide.md".into())))
            );
            assert_eq!(
                text.runs()
                    .iter()
                    .flat_map(|run| &run.styles)
                    .filter(|style| matches!(style, InlineStyle::Image { .. }))
                    .count(),
                2
            );
        }
        let html = serialize_inline_html(text);
        assert!(html.contains("<strong><a href=\"guide.md\"><img src=\"a.png\" alt=\"First\" title=\"Title\"></a></strong>"), "{html}");
    }
    use crate::{Document, EditCommand, InlineFormat};

    #[test]
    fn standalone_linked_images_keep_both_targets_titles_and_undo() {
        let source =
            "[![Résumé](thumb.png \"image title\")](https://example.test/full \"link title\")\n";
        let mut document = Document::from_markdown(source).unwrap();
        let snapshot = document.snapshot();
        let BlockNode::Image(image) = snapshot.blocks().get(0).unwrap().as_ref() else {
            panic!("linked standalone image must be a figure")
        };
        let id = image.id;
        assert_eq!(image.source, "thumb.png");
        assert_eq!(image.alt.as_string(), "Résumé");
        assert_eq!(image.title.as_deref(), Some("image title"));
        let link = image.link.as_ref().unwrap();
        assert_eq!(link.target.0, "https://example.test/full");
        assert_eq!(link.title.as_deref(), Some("link title"));
        assert_eq!(snapshot.serialize().unwrap(), source);
        document
            .apply(EditCommand::SetImageAttributes {
                image_id: id,
                source: "updated.png".into(),
                alt: "Updated description".into(),
            })
            .unwrap();
        let edited = document.snapshot().serialize().unwrap();
        let reopened = Document::from_markdown(edited).unwrap();
        let reopened = reopened.snapshot();
        let BlockNode::Image(image) = reopened.blocks().get(0).unwrap().as_ref() else {
            panic!("edited figure reopens")
        };
        assert_eq!(image.source, "updated.png");
        assert_eq!(image.alt.as_string(), "Updated description");
        assert_eq!(
            image.link.as_ref().unwrap().target.0,
            "https://example.test/full"
        );
        assert_eq!(
            image.link.as_ref().unwrap().title.as_deref(),
            Some("link title")
        );
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        document
            .apply(EditCommand::SetSelection(crate::Selection::Text(
                crate::TextSelection {
                    anchor: DocumentPosition::new(id, 0, Affinity::Downstream),
                    head: DocumentPosition::new(id, "Résumé".len(), Affinity::Upstream),
                },
            )))
            .unwrap();
        document
            .apply(EditCommand::SetLinkSelection {
                target: Some("updated-target.png".into()),
            })
            .unwrap();
        let snapshot = document.snapshot();
        let BlockNode::Image(image) = snapshot.node(id).unwrap() else {
            unreachable!()
        };
        assert_eq!(image.link.as_ref().unwrap().target.0, "updated-target.png");
        assert_eq!(
            image.link.as_ref().unwrap().title.as_deref(),
            Some("link title")
        );
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        document
            .apply(EditCommand::SetLinkSelection { target: None })
            .unwrap();
        let snapshot = document.snapshot();
        let BlockNode::Image(image) = snapshot.node(id).unwrap() else {
            unreachable!()
        };
        assert!(image.link.is_none());
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        let mixed = Document::from_markdown("[![A](a.png) and text](target.md)\n").unwrap();
        assert!(matches!(
            mixed.snapshot().blocks().get(0).unwrap().as_ref(),
            BlockNode::Paragraph(_)
        ));
    }

    #[test]
    fn unchanged_source_is_byte_identical() {
        let source = "---\r\ntitle: X\r\n---\r\n\r\n# Héllo\r\n\r\n<!-- keep -->\r\n\r\nText [ref][x].\r\n\r\n[x]: /url\r\n";
        let document = Document::from_markdown(Arc::<str>::from(source)).expect("valid Markdown");
        assert_eq!(document.snapshot().serialize().expect("serialize"), source);
    }

    #[test]
    fn soft_breaks_reflow_while_hard_breaks_and_original_source_survive() {
        let source = "One source-wrapped\nparagraph.  \nAn intentional break.\n";
        let document = Document::from_markdown(source).unwrap();
        let snapshot = document.snapshot();
        assert_eq!(
            snapshot
                .blocks()
                .get(0)
                .unwrap()
                .text()
                .unwrap()
                .as_string(),
            "One source-wrapped paragraph.  \nAn intentional break."
        );
        assert_eq!(snapshot.serialize().unwrap(), source);
    }

    #[test]
    fn changing_heading_regenerates_only_that_source_unit() {
        let source = "# Old\n\n<!-- exact -->\n\nParagraph  *spacing*.\n";
        let mut document =
            Document::from_markdown(Arc::<str>::from(source)).expect("valid Markdown");
        let heading = document.snapshot().blocks().get(0).expect("heading").id();
        document
            .apply(EditCommand::ReplaceText {
                node_id: heading,
                range: 2..3,
                text: "new".to_owned(),
                selection_after: None,
                typing: false,
            })
            .expect("edit");
        let serialized = document.snapshot().serialize().expect("serialize");
        assert!(serialized.contains("<!-- exact -->\n\nParagraph  *spacing*."));
    }

    #[test]
    fn edited_literal_markdown_text_round_trips_as_plain_text() {
        for literal in [
            "# literal",
            "- literal",
            "1. literal",
            "> literal",
            "`literal`",
            "&copy;",
            "<tag>",
            "https://example.test/a_(b)",
        ] {
            let mut document = Document::from_markdown("seed").expect("document");
            let node_id = document.snapshot().blocks().get(0).expect("paragraph").id();
            document
                .apply(EditCommand::ReplaceText {
                    node_id,
                    range: 0..4,
                    text: literal.into(),
                    selection_after: None,
                    typing: false,
                })
                .expect("replace text");

            let serialized = document.snapshot().serialize().expect("serialize");
            let reparsed = Document::from_markdown(serialized).expect("reparse");
            assert_eq!(reparsed.snapshot().blocks().len(), 1, "{literal:?}");
            assert!(matches!(
                reparsed.snapshot().blocks().get(0).map(Arc::as_ref),
                Some(BlockNode::Paragraph(_))
            ));
            assert_eq!(
                reparsed
                    .snapshot()
                    .blocks()
                    .get(0)
                    .expect("reparsed paragraph")
                    .plain_text(),
                literal,
                "{literal:?}"
            );
        }
    }

    #[test]
    fn edited_code_span_and_complex_link_destination_round_trip() {
        let mut code = Document::from_markdown("seed").expect("code document");
        let code_id = code.snapshot().blocks().get(0).expect("paragraph").id();
        code.apply(EditCommand::ReplaceText {
            node_id: code_id,
            range: 0..4,
            text: "`a`b`".into(),
            selection_after: None,
            typing: false,
        })
        .expect("code text");
        code.apply(EditCommand::ToggleInline {
            node_id: code_id,
            range: 0..5,
            format: InlineFormat::Code,
        })
        .expect("code style");
        let serialized = code.snapshot().serialize().expect("serialize code");
        let reparsed = Document::from_markdown(serialized).expect("reparse code");
        let reparsed_snapshot = reparsed.snapshot();
        let text = reparsed_snapshot.blocks().get(0).expect("code paragraph");
        assert_eq!(text.plain_text(), "`a`b`");
        assert!(
            text.text()
                .expect("rich text")
                .runs()
                .iter()
                .any(|run| { run.styles.contains(&InlineStyle::Code) })
        );

        let target = "https://example.test/a_(b)?q=one &copy;<two>";
        let mut link = Document::from_markdown("link").expect("link document");
        let link_id = link.snapshot().blocks().get(0).expect("paragraph").id();
        link.apply(EditCommand::ToggleInline {
            node_id: link_id,
            range: 0..4,
            format: InlineFormat::Link(target.into()),
        })
        .expect("link style");
        let serialized = link.snapshot().serialize().expect("serialize link");
        let reparsed = Document::from_markdown(serialized).expect("reparse link");
        let reparsed_snapshot = reparsed.snapshot();
        let link_target = reparsed_snapshot
            .blocks()
            .get(0)
            .and_then(|block| block.text())
            .expect("link text")
            .runs()
            .iter()
            .flat_map(|run| run.styles.iter())
            .find_map(|style| match style {
                InlineStyle::Link(LinkTarget(target)) => Some(target.as_str()),
                _ => None,
            });
        assert_eq!(link_target, Some(target));
    }

    #[test]
    fn edited_multiline_inline_code_round_trips_without_whitespace_normalization() {
        let value = "<tag>&\"' λ\nline 2\r\nline 3";
        let mut document = Document::from_markdown("seed").expect("document");
        let node_id = document.snapshot().blocks().get(0).expect("paragraph").id();
        document
            .apply(EditCommand::ReplaceText {
                node_id,
                range: 0..4,
                text: value.into(),
                selection_after: None,
                typing: false,
            })
            .expect("replace text");
        document
            .apply(EditCommand::ToggleInline {
                node_id,
                range: 0..value.len(),
                format: InlineFormat::Code,
            })
            .expect("style code");

        let serialized = document.snapshot().serialize().expect("serialize");
        let reopened = Document::from_markdown(serialized).expect("reopen");
        let snapshot = reopened.snapshot();
        let text = snapshot
            .blocks()
            .get(0)
            .and_then(|block| block.text())
            .expect("paragraph text");

        assert_eq!(text.as_string(), value);
        assert!(
            text.runs()
                .iter()
                .all(|run| run.styles.contains(&InlineStyle::Code))
        );
    }

    #[test]
    fn complex_inline_image_attributes_round_trip() {
        let source = r#"https://example.test/a\b \"quoted\" 'single' (x)?q=&copy;<two>"#;
        let alt = "[résumé] &copy; | literal";
        let title = "quoted \"title\" with \\ slash\nand newline";
        let value = format!("before {alt} after");
        let image_start = "before ".len();
        let image_end = image_start + alt.len();
        let rich = RichText::from_runs(
            value.as_str(),
            vec![
                InlineRun {
                    range: 0..image_start,
                    styles: smallvec::smallvec![],
                },
                InlineRun {
                    range: image_start..image_end,
                    styles: smallvec::smallvec![InlineStyle::Image {
                        source: source.into(),
                        alt: alt.into(),
                        title: Some(title.into()),
                    }],
                },
                InlineRun {
                    range: image_end..value.len(),
                    styles: smallvec::smallvec![],
                },
            ],
        );

        let serialized = serialize_inline(&rich);
        let reopened = Document::from_markdown(serialized.as_str()).expect("reopen image");
        let snapshot = reopened.snapshot();
        let style = snapshot
            .blocks()
            .get(0)
            .and_then(|block| block.text())
            .expect("image paragraph")
            .runs()
            .iter()
            .flat_map(|run| run.styles.iter())
            .find_map(|style| match style {
                InlineStyle::Image { source, alt, title } => Some((source, alt, title)),
                _ => None,
            })
            .unwrap_or_else(|| panic!("image style after {serialized:?}"));

        assert_eq!(style.0, source);
        assert_eq!(style.1, alt);
        assert_eq!(style.2.as_deref(), Some(title));
    }

    #[test]
    fn edited_unicode_and_hard_break_round_trip_exactly() {
        let value = "前🙂e\u{301}  \n後";
        let mut document = Document::from_markdown("seed").expect("document");
        let node_id = document.snapshot().blocks().get(0).expect("paragraph").id();
        document
            .apply(EditCommand::ReplaceText {
                node_id,
                range: 0..4,
                text: value.into(),
                selection_after: None,
                typing: false,
            })
            .expect("replace text");

        let serialized = document.snapshot().serialize().expect("serialize");
        let reopened = Document::from_markdown(serialized).expect("reopen");
        let snapshot = reopened.snapshot();
        let text = snapshot
            .blocks()
            .get(0)
            .and_then(|block| block.text())
            .expect("paragraph text");

        assert_eq!(text.as_string(), value);
        assert!(text.runs().iter().all(|run| run.styles.is_empty()));
    }

    #[test]
    fn adjacent_style_boundaries_round_trip_without_merging_semantics() {
        let rich = RichText::from_runs(
            "abc",
            vec![
                InlineRun {
                    range: 0..1,
                    styles: smallvec::smallvec![InlineStyle::Bold],
                },
                InlineRun {
                    range: 1..2,
                    styles: smallvec::smallvec![InlineStyle::Bold, InlineStyle::Italic],
                },
                InlineRun {
                    range: 2..3,
                    styles: smallvec::smallvec![InlineStyle::Italic],
                },
            ],
        );
        let serialized = serialize_inline(&rich);
        let reparsed = Document::from_markdown(serialized).expect("reparse adjacent styles");
        let snapshot = reparsed.snapshot();
        let text = snapshot
            .blocks()
            .get(0)
            .and_then(|block| block.text())
            .expect("paragraph");
        assert_eq!(text.as_string(), "abc");
        for (offset, bold, italic) in [(0, true, false), (1, true, true), (2, false, true)] {
            let styles = &text
                .runs()
                .iter()
                .find(|run| run.range.contains(&offset))
                .expect("styled byte")
                .styles;
            assert_eq!(styles.contains(&InlineStyle::Bold), bold, "offset {offset}");
            assert_eq!(
                styles.contains(&InlineStyle::Italic),
                italic,
                "offset {offset}"
            );
        }
    }

    #[test]
    fn styled_leading_and_trailing_whitespace_round_trips() {
        let value = " leading and trailing ";
        let rich = RichText::from_runs(
            value,
            vec![InlineRun {
                range: 0..value.len(),
                styles: smallvec::smallvec![InlineStyle::Bold],
            }],
        );

        let serialized = serialize_inline(&rich);
        let reparsed = Document::from_markdown(serialized).expect("reparse styled whitespace");
        let snapshot = reparsed.snapshot();
        let text = snapshot
            .blocks()
            .get(0)
            .and_then(|block| block.text())
            .expect("paragraph");

        assert_eq!(text.as_string(), value);
        assert_eq!(text.runs().len(), 1);
        assert!(text.runs()[0].styles.contains(&InlineStyle::Bold));
    }

    #[test]
    fn crossing_link_and_emphasis_boundaries_round_trip() {
        let target_value = "https://example.test/crossing";
        let target = LinkTarget(target_value.into());
        let rich = RichText::from_runs(
            "abc",
            vec![
                InlineRun {
                    range: 0..1,
                    styles: smallvec::smallvec![InlineStyle::Bold],
                },
                InlineRun {
                    range: 1..2,
                    styles: smallvec::smallvec![
                        InlineStyle::Bold,
                        InlineStyle::Link(target.clone())
                    ],
                },
                InlineRun {
                    range: 2..3,
                    styles: smallvec::smallvec![InlineStyle::Link(target)],
                },
            ],
        );

        let serialized = serialize_inline(&rich);
        let reparsed = Document::from_markdown(serialized).expect("reparse crossing styles");
        let snapshot = reparsed.snapshot();
        let text = snapshot
            .blocks()
            .get(0)
            .and_then(|block| block.text())
            .expect("paragraph");

        assert_eq!(text.as_string(), "abc");
        for (offset, bold, linked) in [(0, true, false), (1, true, true), (2, false, true)] {
            let styles = &text
                .runs()
                .iter()
                .find(|run| run.range.contains(&offset))
                .expect("styled byte")
                .styles;
            assert_eq!(styles.contains(&InlineStyle::Bold), bold, "offset {offset}");
            assert_eq!(
                styles.iter().any(
                    |style| matches!(style, InlineStyle::Link(LinkTarget(value)) if value == target_value)
                ),
                linked,
                "offset {offset}"
            );
        }
    }

    #[test]
    fn editing_code_block_preserves_intentional_trailing_blank_lines() {
        let source = "```rust\nline\n\n\n```\n\nKeep  *exact*.\n";
        let mut document = Document::from_markdown(source).expect("document");
        let code_id = document
            .snapshot()
            .blocks()
            .get(0)
            .expect("code block")
            .id();
        document
            .apply(EditCommand::ReplaceText {
                node_id: code_id,
                range: 0..0,
                text: "edited ".into(),
                selection_after: None,
                typing: false,
            })
            .expect("edit code");
        let expected_content = document
            .snapshot()
            .blocks()
            .get(0)
            .and_then(|block| block.text())
            .expect("code content")
            .as_string();

        let serialized = document.snapshot().serialize().expect("serialize");
        assert!(serialized.contains("line\n\n\n```"));
        assert!(serialized.contains("Keep  *exact*."));
        let reparsed = Document::from_markdown(serialized).expect("reparse");
        assert_eq!(
            reparsed
                .snapshot()
                .blocks()
                .get(0)
                .and_then(|block| block.text())
                .expect("reparsed code")
                .as_string(),
            expected_content
        );
    }

    #[test]
    fn structural_edit_keeps_untouched_source_units_and_block_boundaries() {
        let source = "Title\n=====\n\none\n\ntwo  *spacing*.\n";
        let mut document = Document::from_markdown(source).expect("document");
        let initial = document.snapshot();
        let heading = initial.blocks().get(0).expect("heading").id();
        assert!(
            initial.source_spine().unit(heading).is_some(),
            "setext heading needs an owned source unit"
        );
        let one = initial.blocks().get(1).expect("one").id();
        document
            .apply(EditCommand::SetSelection(Selection::Text(
                TextSelection::caret(DocumentPosition::new(one, 1, Affinity::Downstream)),
            )))
            .expect("selection");
        document
            .apply(EditCommand::SplitSelection)
            .expect("split paragraph");

        let edited = document.snapshot();
        assert!(
            !edited.dirty_node_ids().contains(&heading),
            "untouched heading was marked dirty: {:?}",
            edited.dirty_node_ids()
        );
        let serialized = edited.serialize().expect("serialize");
        assert!(
            serialized.starts_with("Title\n=====\n"),
            "serialized source: {serialized:?}"
        );
        assert!(serialized.contains("two  *spacing*."));
        let reparsed = Document::from_markdown(serialized).expect("reparse");
        assert_eq!(reparsed.snapshot().blocks().len(), 4);
        assert_eq!(
            reparsed
                .snapshot()
                .blocks()
                .get(1)
                .expect("left")
                .plain_text(),
            "o"
        );
        assert_eq!(
            reparsed
                .snapshot()
                .blocks()
                .get(2)
                .expect("right")
                .plain_text(),
            "ne"
        );
    }

    #[test]
    fn deleted_block_gaps_do_not_accumulate_and_surviving_spacing_stays_exact() {
        for newline in ["\n", "\r\n"] {
            for trailing in [false, true] {
                let source = if trailing {
                    "Before\n\nRemove one\n\nRemove two\n"
                } else {
                    "Before\n\nRemove one\n\nRemove two\n\n\nAfter  *exact*\n"
                }
                .replace('\n', newline);
                let mut document = Document::from_markdown(source.as_str()).unwrap();
                let ids = document
                    .snapshot()
                    .blocks()
                    .iter()
                    .skip(1)
                    .take(2)
                    .map(|block| block.id())
                    .collect::<Vec<_>>();
                for node_id in ids {
                    document
                        .apply(EditCommand::DeleteBlock { node_id })
                        .unwrap();
                }
                let expected = if trailing {
                    "Before\n"
                } else {
                    "Before\n\n\nAfter  *exact*\n"
                }
                .replace('\n', newline);
                assert_eq!(document.snapshot().serialize().unwrap(), expected);
                document.undo().unwrap();
                document.undo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        }
    }

    #[test]
    fn generated_block_uses_existing_separator_without_losing_definitions() {
        for newline in ["\n", "\r\n"] {
            for source in [
                "one\n\n\nAfter  *exact*\n",
                "one\n\n[ref]: /destination \"Title\"\n\nAfter [link][ref]\n",
            ] {
                let source = source.replace('\n', newline);
                let mut document = Document::from_markdown(source.as_str()).unwrap();
                let first = document.snapshot().blocks().get(0).unwrap().id();
                document
                    .apply(EditCommand::SetSelection(Selection::Text(
                        TextSelection::caret(DocumentPosition::new(first, 1, Affinity::Downstream)),
                    )))
                    .unwrap();
                document.apply(EditCommand::SplitSelection).unwrap();
                let expected = source.replacen("one", &format!("o{newline}{newline}ne"), 1);
                assert_eq!(document.snapshot().serialize().unwrap(), expected);
                let reparsed = Document::from_markdown(expected).unwrap();
                assert_eq!(reparsed.snapshot().blocks().len(), 3);
                document.undo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        }
    }

    #[test]
    fn deleting_a_neighbor_keeps_reference_definition_source() {
        let source = concat!(
            "Keep [reference][ref].\n\n",
            "[ref]: /exact-destination \"Exact title\"\n\n",
            "Delete me.\n\n",
            "Tail.\n"
        );
        let mut document = Document::from_markdown(source).expect("document");
        assert_eq!(document.snapshot().blocks().len(), 3);
        let deleted = document.snapshot().blocks().get(1).expect("neighbor").id();
        document
            .apply(EditCommand::DeleteBlock { node_id: deleted })
            .expect("delete block");

        let serialized = document.snapshot().serialize().expect("serialize");
        assert!(serialized.contains("[ref]: /exact-destination \"Exact title\""));
        assert!(!serialized.contains("Delete me."));
        assert!(serialized.contains("Keep [reference][ref]."));
        let reparsed = Document::from_markdown(serialized).expect("reparse");
        let snapshot = reparsed.snapshot();
        assert_eq!(snapshot.blocks().len(), 2);
        let first = snapshot
            .blocks()
            .get(0)
            .and_then(|block| block.text())
            .expect("linked paragraph");
        assert!(first.runs().iter().any(|run| {
            run.styles.iter().any(|style| {
                matches!(style, InlineStyle::Link(LinkTarget(target)) if target == "/exact-destination")
            })
        }));
    }

    #[test]
    fn generated_structural_boundaries_follow_crlf_source() {
        let mut document = Document::from_markdown("one\r\n\r\ntwo\r\n").expect("document");
        let first = document.snapshot().blocks().get(0).expect("first").id();
        document
            .apply(EditCommand::SetSelection(Selection::Text(
                TextSelection::caret(DocumentPosition::new(first, 1, Affinity::Downstream)),
            )))
            .expect("selection");
        document
            .apply(EditCommand::SplitSelection)
            .expect("split paragraph");

        let serialized = document.snapshot().serialize().expect("serialize");
        assert!(!serialized.replace("\r\n", "").contains('\n'));
        let reparsed = Document::from_markdown(serialized).expect("reparse");
        assert_eq!(reparsed.snapshot().blocks().len(), 3);
    }

    #[test]
    fn unrelated_edit_keeps_footnote_definition_in_source_order_once() {
        let source = concat!(
            "# Heading\n\n",
            "Text with a note.[^source]\n\n",
            "[^source]: Exact footnote source.\n\n",
            "<details><summary>Later HTML</summary>keep</details>\n"
        );
        let mut document = Document::from_markdown(source).expect("document");
        let heading = document.snapshot().blocks().get(0).expect("heading").id();
        document
            .apply(EditCommand::ReplaceText {
                node_id: heading,
                range: 0..0,
                text: "Edited ".into(),
                selection_after: None,
                typing: false,
            })
            .expect("edit heading");

        let serialized = document.snapshot().serialize().expect("serialize");
        assert_eq!(
            serialized,
            source.replacen("# Heading", "# Edited Heading", 1)
        );
        assert_eq!(serialized.matches("[^source]:").count(), 1);
    }

    #[test]
    fn imports_all_required_gfm_extensions() {
        let source =
            "- [x] done\n\n~~strike~~ https://example.com\n\n|a|b|\n|-|-|\n|c|d|\n\n[^x]: note\n";
        let document = Document::from_markdown(Arc::<str>::from(source)).expect("GFM imports");
        assert!(document.snapshot().blocks().iter().any(|block| {
            matches!(
                block.as_ref(),
                BlockNode::List(list)
                    if list.items.len() == 1
                        && list.items[0].checked == Some(true)
                        && list.items[0].blocks.get(0).is_some_and(|block| block.plain_text() == "done")
            )
        }));
        assert!(
            document
                .snapshot()
                .blocks()
                .iter()
                .any(|block| matches!(block.as_ref(), BlockNode::Table(_)))
        );
        assert_eq!(document.snapshot().serialize().expect("serialize"), source);
    }

    #[test]
    fn default_table_rule_and_explicit_border_units_survive_cell_edits() {
        let body = "| Key | Value |\n| --- | --- |\n| a | b |\n";
        for border in [
            None,
            Some(crate::TableBorder::PhysicalPixel),
            Some(crate::TableBorder::LogicalPixel),
            Some(crate::TableBorder::Dotted),
            Some(crate::TableBorder::None),
        ] {
            let source = border.map_or_else(|| body.to_owned(), |border|
                format!("<!-- tachyon-table:v1 {{\"border\":{},\"widths\":[null,null]}} -->\n{body}", serde_json::to_string(&border).unwrap()));
            let mut document = Document::from_markdown(source.clone()).unwrap();
            let snapshot = document.snapshot();
            let BlockNode::Table(table) = snapshot.blocks().get(0).unwrap().as_ref() else {
                panic!("table")
            };
            assert_eq!(
                table.border,
                border.unwrap_or(crate::TableBorder::LogicalPixel)
            );
            assert_eq!(snapshot.serialize().unwrap(), source);
            let node = table.rows[1].cells[0].blocks.get(0).unwrap().id();
            document
                .apply(EditCommand::ReplaceText {
                    node_id: node,
                    range: 0..1,
                    text: "changed".into(),
                    selection_after: None,
                    typing: false,
                })
                .unwrap();
            let serialized = document.snapshot().serialize().unwrap();
            if border.is_none() {
                assert!(
                    !serialized.contains("tachyon-table"),
                    "plain Markdown needs no presentation annotation"
                );
            }
            let reparsed = Document::from_markdown(serialized).unwrap();
            let snapshot = reparsed.snapshot();
            let BlockNode::Table(table) = snapshot.blocks().get(0).unwrap().as_ref() else {
                panic!("table")
            };
            assert_eq!(
                table.border,
                border.unwrap_or(crate::TableBorder::LogicalPixel)
            );
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        }
    }

    #[test]
    fn table_metadata_round_trips_width_border_and_unknown_fields() {
        let source = concat!(
            "<!-- tachyon-table:v1 {\"border\":\"PhysicalPixel\",",
            "\"widths\":[120.0,null],\"future\":{\"mode\":7}} -->\n",
            "| a | b |\n| --- | --- |\n| c | d |\n"
        );
        let mut document = Document::from_markdown(source).expect("metadata table imports");
        let snapshot = document.snapshot();
        assert_eq!(snapshot.blocks().len(), 1);
        let BlockNode::Table(table) = snapshot.blocks().get(0).expect("table").as_ref() else {
            panic!("metadata comment must be attached to the table");
        };
        assert_eq!(table.border, crate::TableBorder::PhysicalPixel);
        assert_eq!(table.columns[0].width, Some(120.));
        assert!(
            table
                .preserved_metadata
                .iter()
                .any(|value| value.contains("future"))
        );
        let cell_text = table.rows[1].cells[0]
            .blocks
            .get(0)
            .expect("cell paragraph")
            .id();
        document
            .apply(EditCommand::ReplaceText {
                node_id: cell_text,
                range: 0..1,
                text: "changed".into(),
                selection_after: None,
                typing: false,
            })
            .expect("cell edit");
        let serialized = document.snapshot().serialize().expect("serialize");
        assert!(serialized.contains("\"border\":\"PhysicalPixel\""));
        assert!(serialized.contains("\"widths\":[120.0,null]"));
        assert!(serialized.contains("\"future\":{\"mode\":7}"));
        let reparsed = Document::from_markdown(serialized).expect("reparse metadata");
        assert_eq!(reparsed.snapshot().blocks().len(), 1);
    }

    #[test]
    fn table_metadata_edit_preserves_rich_inline_cells_and_literal_pipes() {
        let source = concat!(
            "| Rich | More |\n",
            "| --- | --- |\n",
            "| **bold** and [link](https://example.com) | `a\\|b` |\n"
        );
        let mut document = Document::from_markdown(source).expect("table imports");
        let table_id = document.snapshot().blocks().get(0).expect("table").id();
        document
            .apply(EditCommand::SetTableBorder {
                table_id,
                border: crate::TableBorder::PhysicalPixel,
            })
            .expect("metadata edit");

        let serialized = document.snapshot().serialize().expect("serialize");
        assert!(serialized.contains("**bold**"));
        assert!(serialized.contains("[link](https://example.com)"));
        assert!(serialized.contains("`a\\|b`"));
        let reparsed = Document::from_markdown(serialized).expect("reparse table");
        let reparsed_snapshot = reparsed.snapshot();
        let BlockNode::Table(table) = reparsed_snapshot
            .blocks()
            .get(0)
            .expect("reparsed table")
            .as_ref()
        else {
            panic!("reparsed block must be a table");
        };
        let first = table.rows[1].cells[0].blocks.get(0).expect("rich cell");
        let BlockNode::Paragraph(first) = first.as_ref() else {
            panic!("rich cell must contain a paragraph");
        };
        assert!(first.content.runs().iter().any(|run| {
            run.styles
                .iter()
                .any(|style| matches!(style, InlineStyle::Bold))
        }));
        assert!(first.content.runs().iter().any(|run| {
            run.styles
                .iter()
                .any(|style| matches!(style, InlineStyle::Link(_)))
        }));
        let second = table.rows[1].cells[1].blocks.get(0).expect("code cell");
        assert_eq!(second.plain_text(), "a|b");
    }

    #[test]
    fn table_presentation_edits_preserve_complete_gfm_inline_semantics() {
        let source = concat!(
            "| Rich | Code and break | Empty |\r\n",
            "| :--- | :---: | ---: |\r\n",
            "| **bold** ~~strike~~ [link](https://example.test/a) ",
            "![alt](image.png \"Title\") | ``tick ` value`` and `a\\|b` <br> next | |\r\n"
        );
        let mut document = Document::from_markdown(source).expect("table");
        let snapshot = document.snapshot();
        let BlockNode::Table(table) = snapshot.blocks().get(0).expect("table").as_ref() else {
            panic!("table")
        };
        let table_id = table.id;
        let before_cells = table.rows[1]
            .cells
            .iter()
            .map(|cell| cell.blocks.get(0).expect("paragraph").plain_text())
            .collect::<Vec<_>>();
        assert!(before_cells[1].contains('\n'), "imported hard break");

        document
            .apply(EditCommand::SetTableBorder {
                table_id,
                border: crate::TableBorder::Dotted,
            })
            .expect("border");
        document
            .apply(EditCommand::SetTableColumnWidth {
                table_id,
                column: 0,
                width: 180.0,
            })
            .expect("width");
        document
            .apply(EditCommand::SetTableColumnAlignment {
                table_id,
                column: 2,
                alignment: ColumnAlignment::Center,
            })
            .expect("alignment");

        let serialized = document.snapshot().serialize().expect("serialize");
        assert!(
            !serialized.contains("<table>"),
            "inline cells remain GFM: {serialized}"
        );
        assert!(
            !serialized.replace("\r\n", "").contains('\n'),
            "generated table follows the CRLF document: {serialized:?}"
        );
        let reopened = Document::from_markdown(serialized).expect("reopen table");
        let reopened_snapshot = reopened.snapshot();
        let BlockNode::Table(table) = reopened_snapshot
            .blocks()
            .get(0)
            .expect("reopened table")
            .as_ref()
        else {
            panic!("reopened table")
        };
        assert_eq!(table.border, crate::TableBorder::Dotted);
        assert_eq!(table.columns[0].width, Some(180.0));
        assert_eq!(table.columns[2].alignment, ColumnAlignment::Center);
        assert_eq!(
            table.rows[1]
                .cells
                .iter()
                .map(|cell| cell.blocks.get(0).expect("paragraph").plain_text())
                .collect::<Vec<_>>(),
            before_cells
        );
        let rich = table.rows[1].cells[0]
            .blocks
            .get(0)
            .unwrap()
            .text()
            .unwrap();
        assert!(
            rich.runs()
                .iter()
                .any(|run| run.styles.contains(&InlineStyle::Bold))
        );
        assert!(
            rich.runs()
                .iter()
                .any(|run| run.styles.contains(&InlineStyle::Strikethrough))
        );
        assert!(rich.runs().iter().any(|run| run.styles.iter().any(
            |style| matches!(style, InlineStyle::Link(LinkTarget(target)) if target == "https://example.test/a")
        )));
        assert!(rich.runs().iter().any(|run| run.styles.iter().any(
            |style| matches!(style, InlineStyle::Image { source, alt, title }
                if source == "image.png" && alt == "alt" && title.as_deref() == Some("Title"))
        )));
        let code = table.rows[1].cells[1]
            .blocks
            .get(0)
            .unwrap()
            .text()
            .unwrap();
        assert!(
            code.runs()
                .iter()
                .filter(|run| run.styles.contains(&InlineStyle::Code))
                .count()
                >= 2
        );
    }

    #[test]
    fn block_rich_html_table_reopens_with_metadata_and_alignment() {
        let source = concat!(
            "<!-- tachyon-table:v1 {\"border\":\"PhysicalPixel\",",
            "\"widths\":[120.0,null],\"future\":\"\\u002d\\u002d>\"} -->\n",
            "| Head | More |\n",
            "| --- | --- |\n",
            "| **alpha** | [beta](/destination) |\n"
        );
        let mut document = Document::from_markdown(source).expect("table imports");
        let snapshot = document.snapshot();
        let BlockNode::Table(table) = snapshot.blocks().get(0).expect("table").as_ref() else {
            panic!("table block");
        };
        let table_id = table.id;
        let cell_text = table.rows[1].cells[0]
            .blocks
            .get(0)
            .expect("cell paragraph")
            .id();
        document
            .apply(EditCommand::SetSelection(Selection::Text(
                TextSelection::caret(DocumentPosition::new(cell_text, 2, Affinity::Downstream)),
            )))
            .expect("cell selection");
        document
            .apply(EditCommand::SplitSelection)
            .expect("second cell paragraph");
        document
            .apply(EditCommand::SetTableColumnAlignment {
                table_id,
                column: 1,
                alignment: ColumnAlignment::Right,
            })
            .expect("column alignment");

        let serialized = document.snapshot().serialize().expect("serialize");
        assert!(serialized.contains("<table>"));
        assert!(serialized.contains("<th align=\"right\">"));
        assert_eq!(serialized.matches("-->").count(), 1);
        let reparsed = Document::from_markdown(serialized.clone()).expect("reparse HTML table");
        let reparsed_snapshot = reparsed.snapshot();
        let BlockNode::Table(table) = reparsed_snapshot
            .blocks()
            .get(0)
            .expect("reparsed table")
            .as_ref()
        else {
            panic!("reparsed table block");
        };
        assert_eq!(table.border, crate::TableBorder::PhysicalPixel);
        assert_eq!(table.columns[0].width, Some(120.));
        assert_eq!(table.columns[1].alignment, ColumnAlignment::Right);
        assert_eq!(
            table.rows[1].cells[0].blocks.len(),
            2,
            "serialized table: {serialized:?}"
        );
        assert!(
            table
                .preserved_metadata
                .iter()
                .any(|value| value.contains("future"))
        );

        assert!(document.rebase_source(document.snapshot().prepare_source_rebase().unwrap()));
        let snapshot = document.snapshot();
        let BlockNode::Table(table) = snapshot.node(table_id).expect("edited table") else {
            panic!("edited table")
        };
        let blocks = &table.rows[1].cells[0].blocks;
        let first = blocks.get(0).expect("left paragraph");
        let second = blocks.get(1).expect("right paragraph");
        let join = TextSelection {
            anchor: DocumentPosition::new(
                first.id(),
                first.text().expect("left text").len(),
                Affinity::Downstream,
            ),
            head: DocumentPosition::new(second.id(), 0, Affinity::Upstream),
        };
        document
            .apply(EditCommand::SetSelection(Selection::Text(join)))
            .expect("join selection");
        document
            .apply(EditCommand::ReplaceSelection {
                text: String::new(),
                typing: false,
            })
            .expect("remove extra block");
        let compact = document.snapshot().serialize().expect("compact table");
        assert!(
            !compact.contains("<table>"),
            "single paragraphs use GFM: {compact}"
        );
        assert!(document.rebase_source(document.snapshot().prepare_source_rebase().unwrap()));
        let compact_document = Document::from_markdown(compact).expect("reopen compact table");
        let compact_snapshot = compact_document.snapshot();
        let BlockNode::Table(table) = compact_snapshot.blocks().get(0).unwrap().as_ref() else {
            panic!("compact table")
        };
        assert_eq!(table.border, crate::TableBorder::PhysicalPixel);
        assert_eq!(table.columns[0].width, Some(120.));
        assert_eq!(table.columns[1].alignment, ColumnAlignment::Right);
        assert_eq!(table.rows[1].cells[0].blocks.len(), 1);
        assert_eq!(
            table.rows[1].cells[0]
                .blocks
                .get(0)
                .expect("compacted paragraph")
                .plain_text(),
            "alpha"
        );
        assert!(
            table
                .preserved_metadata
                .iter()
                .any(|value| value.contains("future"))
        );
    }

    #[test]
    fn authored_html_table_retains_header_rows_rich_blocks_and_unknown_metadata() {
        let source = concat!(
            "<!-- tachyon-table:v1 {\"border\":\"Dotted\",",
            "\"widths\":[140.0,null],\"future\":{\"mode\":7}} -->\n",
            "<table>\n",
            "<tr><th align=\"center\">Primary</th><th>Other</th></tr>\n",
            "<tr><th>Detail</th><th>More</th></tr>\n",
            "<tr><td><h3>Title</h3><p><strong>bold</strong> ",
            "<a href=\"https://example.test\">link</a> ",
            "<img src=\"image.png\" alt=\"alt\"></p>",
            "<pre><code class=\"language-rust\">let x = 1;</code></pre>",
            "<ul><li>one</li><li>two</li></ul></td><td><p>Side</p></td></tr>\n",
            "</table>\n"
        );
        let mut document = Document::from_markdown(source).expect("HTML table");
        let table_id = document.snapshot().blocks().get(0).expect("table").id();
        document
            .apply(EditCommand::SetTableColumnWidth {
                table_id,
                column: 1,
                width: 220.0,
            })
            .expect("width-only edit");

        let saved = document.snapshot().serialize().expect("serialize table");
        assert_eq!(saved.matches("-->").count(), 1, "safe metadata: {saved}");
        let reopened = Document::from_markdown(saved).expect("reopen table");
        let snapshot = reopened.snapshot();
        let BlockNode::Table(table) = snapshot.blocks().get(0).expect("table").as_ref() else {
            panic!("table")
        };
        assert_eq!(table.header_rows, 2);
        assert_eq!(table.border, crate::TableBorder::Dotted);
        assert_eq!(table.columns[0].alignment, ColumnAlignment::Center);
        assert_eq!(table.columns[0].width, Some(140.0));
        assert_eq!(table.columns[1].width, Some(220.0));
        assert!(
            table
                .preserved_metadata
                .iter()
                .any(|value| value.contains("future"))
        );

        let blocks = &table.rows[2].cells[0].blocks;
        assert!(matches!(
            blocks.get(0).map(Arc::as_ref),
            Some(BlockNode::Heading(_))
        ));
        let paragraph = blocks
            .get(1)
            .and_then(|block| block.text())
            .expect("paragraph");
        assert!(
            paragraph
                .runs()
                .iter()
                .any(|run| run.styles.contains(&InlineStyle::Bold))
        );
        assert!(paragraph.runs().iter().any(|run| run.styles.iter().any(
            |style| matches!(style, InlineStyle::Link(LinkTarget(target)) if target == "https://example.test")
        )));
        assert!(paragraph
            .runs()
            .iter()
            .any(|run| run.styles.iter().any(|style| matches!(style, InlineStyle::Image { source, alt, .. } if source == "image.png" && alt == "alt"))));
        assert!(
            matches!(blocks.get(2).map(Arc::as_ref), Some(BlockNode::CodeBlock(code)) if code.language.as_deref() == Some("rust") && code.content.as_string() == "let x = 1;\n")
        );
        assert!(
            matches!(blocks.get(3).map(Arc::as_ref), Some(BlockNode::List(list)) if list.items.len() == 2),
            "rich cell blocks: {blocks:?}"
        );
    }

    #[test]
    fn display_math_import_edit_and_reopen_preserve_literal_source() {
        for source in [
            "Before\n\n$$\n\\frac{1}{2}\n$$\n\nAfter\n",
            "$$x^2$$\n",
            "> $$x+1$$\n",
        ] {
            let mut document = Document::from_markdown(source).unwrap();
            let snapshot = document.snapshot();
            assert_eq!(snapshot.serialize().unwrap(), source);
            let projection_block = snapshot
                .blocks()
                .iter()
                .find_map(|block| match block.as_ref() {
                    BlockNode::CodeBlock(code) => Some(code.clone()),
                    BlockNode::BlockQuote { blocks, .. } => blocks.iter().find_map(|child| {
                        if let BlockNode::CodeBlock(code) = child.as_ref() {
                            Some(code.clone())
                        } else {
                            None
                        }
                    }),
                    _ => None,
                })
                .expect("standalone display math is an editable literal block");
            assert_eq!(projection_block.syntax, crate::CodeBlockSyntax::DisplayMath);
            let original = projection_block.content.as_string();
            document
                .apply(crate::EditCommand::ReplaceText {
                    node_id: projection_block.id,
                    range: 0..0,
                    text: "y+".into(),
                    selection_after: None,
                    typing: false,
                })
                .unwrap();
            let saved = document.snapshot().serialize().unwrap();
            assert!(
                saved.contains("$$y+"),
                "display delimiter retained: {saved}"
            );
            let reopened = Document::from_markdown(saved.as_str()).unwrap();
            // IDs are import-local; compare content recursively rather than
            // assigning identity by matching text or original source offset.
            fn contents(block: &BlockNode) -> Option<String> {
                match block {
                    BlockNode::CodeBlock(code) => Some(code.content.as_string()),
                    BlockNode::BlockQuote { blocks, .. } => blocks.iter().find_map(|b| contents(b)),
                    _ => None,
                }
            }
            assert_eq!(
                reopened
                    .snapshot()
                    .blocks()
                    .iter()
                    .find_map(|b| contents(b))
                    .unwrap(),
                format!("y+{original}")
            );
            assert_eq!(document.undo().unwrap().serialize().unwrap(), source);
        }
    }

    #[test]
    fn display_math_falls_back_to_a_lossless_fence_when_an_edit_inserts_delimiters() {
        let mut document = Document::from_markdown("$$x$$\n").unwrap();
        let id = document.snapshot().blocks().get(0).unwrap().id();
        document
            .apply(crate::EditCommand::ReplaceText {
                node_id: id,
                range: 0..1,
                text: "x $$ y".into(),
                selection_after: None,
                typing: false,
            })
            .unwrap();
        let saved = document.snapshot().serialize().unwrap();
        assert!(saved.starts_with("```math"));
        let reopened = Document::from_markdown(saved.as_str()).unwrap();
        assert_eq!(
            reopened
                .snapshot()
                .blocks()
                .get(0)
                .unwrap()
                .text()
                .unwrap()
                .as_string(),
            "x $$ y\n"
        );
        assert_eq!(document.undo().unwrap().serialize().unwrap(), "$$x$$\n");
    }

    #[test]
    fn inline_math_survives_an_unrelated_edit_and_reparse() {
        let source = "Value $x^2 + \\alpha$ and $$y+1$$ remain literal.\n";
        let mut document = Document::from_markdown(source).unwrap();
        let snapshot = document.snapshot();
        let block = snapshot.blocks().get(0).unwrap();
        let text = block.text().unwrap();
        assert!(
            text.runs()
                .iter()
                .any(|run| run.styles.contains(&InlineStyle::Math { display: false }))
        );
        assert!(
            text.runs()
                .iter()
                .any(|run| run.styles.contains(&InlineStyle::Math { display: true }))
        );
        document
            .apply(crate::EditCommand::ReplaceText {
                node_id: block.id(),
                range: 0..0,
                text: "A ".into(),
                selection_after: None,
                typing: false,
            })
            .unwrap();
        let saved = document.snapshot().serialize().unwrap();
        assert!(saved.contains("$x^2 + \\alpha$"));
        assert!(saved.contains("$$y+1$$"));
        let reopened = Document::from_markdown(saved.as_str()).unwrap();
        assert_eq!(
            reopened
                .snapshot()
                .blocks()
                .get(0)
                .unwrap()
                .text()
                .unwrap()
                .as_string(),
            document
                .snapshot()
                .blocks()
                .get(0)
                .unwrap()
                .text()
                .unwrap()
                .as_string()
        );
    }

    #[test]
    fn malformed_table_metadata_remains_preserved_source() {
        for prefix in [
            "<!-- tachyon-table:v1 {\"border\":\"Dotted\",\"widths\":[-1]} -->\n",
            "<!-- tachyon-table:v2 {\"border\":\"Dotted\",\"widths\":[120]} -->\n",
            "<!-- tachyon-table:v1 not-json -->\n",
            "<!-- tachyon-table:v1 {\"widths\":[120]} -->\n",
            "<!-- tachyon-table:v1 {\"border\":\"Dotted\",\"widths\":[]} -->\n",
        ] {
            for body in [
                "| a |\n| --- |\n| b |\n",
                "<table><tr><td><p>Body</p><p>Second</p></td></tr></table>\n",
            ] {
                for newline in ["\n", "\r\n"] {
                    let prefix = prefix.replace('\n', newline);
                    let source = format!("{prefix}{}", body.replace('\n', newline));
                    let mut document =
                        Document::from_markdown(source.clone()).expect("document imports");
                    assert_eq!(document.snapshot().blocks().len(), 2);
                    assert!(matches!(
                        document.snapshot().blocks().get(0).map(Arc::as_ref),
                        Some(BlockNode::PreservedSource { .. })
                    ));
                    assert_eq!(document.snapshot().serialize().expect("serialize"), source);

                    let table_id = document.snapshot().blocks().get(1).expect("table").id();
                    document
                        .apply(EditCommand::SetTableBorder {
                            table_id,
                            border: crate::TableBorder::Dotted,
                        })
                        .expect("presentation edit");
                    let saved = document.snapshot().serialize().expect("changed table");
                    assert!(
                        saved.starts_with(&prefix),
                        "opaque metadata changed: {saved}"
                    );
                    let reopened = Document::from_markdown(saved).expect("reopen table");
                    assert!(reopened.snapshot().blocks().iter().any(
                |block| matches!(block.as_ref(), BlockNode::Table(table) if table.border == crate::TableBorder::Dotted)
            ));
                }
            }
        }
    }
}
