use document_core::{
    Affinity, BlockNode, Document, DocumentPosition, EditCommand, Selection, TextSelection,
};

fn assert_valid(document: &Document) {
    let snapshot = document.snapshot();
    match snapshot.selection() {
        Selection::Text(selection) => {
            assert!(snapshot.validates_position(selection.anchor));
            assert!(snapshot.validates_position(selection.head));
        }
        Selection::Table(selection) => {
            let Some(BlockNode::Table(table)) = snapshot.node(selection.table_id) else {
                panic!("selected table must exist")
            };
            for (row, column) in [
                (selection.anchor_row, selection.anchor_column),
                (selection.head_row, selection.head_column),
            ] {
                assert!(
                    table
                        .rows
                        .get(row)
                        .and_then(|row| row.cells.get(column))
                        .is_some()
                );
            }
        }
    }
}

fn first_text(block: &BlockNode) -> Option<DocumentPosition> {
    if let Some(text) = block.text() {
        return Some(DocumentPosition::new(
            block.id(),
            text.len(),
            Affinity::Upstream,
        ));
    }
    match block {
        BlockNode::List(list) => list
            .items
            .iter()
            .flat_map(|item| item.blocks.iter())
            .find_map(|block| first_text(block)),
        BlockNode::Table(table) => table
            .rows
            .iter()
            .flat_map(|row| row.cells.iter())
            .flat_map(|cell| cell.blocks.iter())
            .find_map(|block| first_text(block)),
        BlockNode::BlockQuote { blocks, .. } | BlockNode::Alert { blocks, .. } => {
            blocks.iter().find_map(|block| first_text(block))
        }
        _ => None,
    }
}

#[test]
fn composition_publication_and_history_keep_valid_carets() {
    for source in ["Ωx\n", "> Ωx\n", "- Ωx\n", "| Ωx |\n| --- |\n| Body |\n"] {
        for commit in [false, true] {
            let mut document = Document::from_markdown(source).unwrap();
            let initial = document.snapshot();
            let root = initial.blocks().get(0).unwrap();
            let position = first_text(root).unwrap();
            let range = TextSelection {
                anchor: position,
                head: DocumentPosition::new(position.node_id, 0, Affinity::Downstream),
            };
            document
                .apply(EditCommand::SetSelection(Selection::Text(range.clone())))
                .unwrap();
            let before = document.snapshot();
            let invalid = TextSelection::caret(DocumentPosition::new(
                position.node_id,
                1,
                Affinity::Downstream,
            ));
            assert!(document.begin_composition(invalid).is_err());
            assert!(!document.composition_active());
            assert_eq!(document.snapshot().revision(), before.revision());
            document.begin_composition(range).unwrap();
            for text in ["", "候", "候補"] {
                document.update_composition(text.into()).unwrap();
                assert_valid(&document);
                let current = document.snapshot();
                assert!(
                    document
                        .apply(EditCommand::DeleteBlock { node_id: root.id() })
                        .is_err()
                );
                assert_eq!(document.snapshot().revision(), current.revision());
                assert_eq!(
                    document.snapshot().serialize().unwrap(),
                    current.serialize().unwrap()
                );
            }
            if commit {
                document.commit_composition().unwrap();
                assert_valid(&document);
                let saved = document.snapshot().serialize().unwrap();
                document.undo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), source);
                assert_eq!(document.snapshot().selection(), before.selection());
                document.redo().unwrap();
                assert_valid(&document);
                assert_eq!(document.snapshot().serialize().unwrap(), saved);
            } else {
                document.cancel_composition().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), source);
                assert_eq!(document.snapshot().selection(), before.selection());
            }
            document
                .apply(EditCommand::ReplaceSelection {
                    text: "✓".into(),
                    typing: false,
                })
                .unwrap();
            assert_valid(&document);
            assert!(document.snapshot().serialize().unwrap().contains('✓'));
        }
    }
}

#[test]
fn explicit_post_edit_endpoints_are_validated_atomically() {
    for invalid in 0..3 {
        let mut document = Document::from_markdown("old\n\n---\n").unwrap();
        let before = document.snapshot();
        let id = before.blocks().get(0).unwrap().id();
        let node = if invalid == 2 {
            before.blocks().get(1).unwrap().id()
        } else {
            id
        };
        let offset = match invalid {
            0 => 1,
            1 => 99,
            _ => 0,
        };
        let result = document.apply(EditCommand::ReplaceText {
            node_id: id,
            range: 0..3,
            text: "Ωx".into(),
            typing: false,
            selection_after: Some(Selection::Text(TextSelection::caret(
                DocumentPosition::new(node, offset, Affinity::Upstream),
            ))),
        });
        assert!(result.is_err());
        assert_eq!(document.snapshot().revision(), before.revision());
        assert_eq!(
            document.snapshot().serialize().unwrap(),
            before.serialize().unwrap()
        );
        assert_eq!(document.snapshot().selection(), before.selection());
        assert_valid(&document);
        let selection = TextSelection {
            anchor: DocumentPosition::new(id, 3, Affinity::Upstream),
            head: DocumentPosition::new(id, 0, Affinity::Downstream),
        };
        document
            .apply(EditCommand::ReplaceText {
                node_id: id,
                range: 0..3,
                text: "Ωx".into(),
                typing: false,
                selection_after: Some(Selection::Text(selection.clone())),
            })
            .unwrap();
        assert_eq!(document.snapshot().selection(), &Selection::Text(selection));
        assert_valid(&document);
        document.undo().unwrap();
        assert_eq!(
            document.snapshot().serialize().unwrap(),
            before.serialize().unwrap()
        );
        assert_eq!(document.snapshot().selection(), before.selection());
    }
}

#[test]
fn deleting_active_containers_uses_neighbors_or_a_transient_caret() {
    for body in [
        "Ωx\n",
        "> Ωx\n",
        "> [!NOTE]\n> Ωx\n",
        "- Ωx\n",
        "```rust\nΩx\n```\n",
        "| H |\n| --- |\n| Ωx |\n",
    ] {
        for (prefix, suffix) in [
            ("Before\n\n", "\nAfter\n"),
            ("Before\n\n", ""),
            ("", "\nAfter\n"),
            ("<!-- keep -->\n\n", ""),
            ("", ""),
        ] {
            for newline in ["\n", "\r\n"] {
                for rectangle in [false, true] {
                    if rectangle && !body.starts_with('|') {
                        continue;
                    }
                    let source = format!("{prefix}{body}{suffix}").replace('\n', newline);
                    let mut document = Document::from_markdown(source.as_str()).unwrap();
                    let initial = document.snapshot();
                    let root = initial
                        .blocks()
                        .get(usize::from(!prefix.is_empty()))
                        .unwrap();
                    let expected = initial
                        .blocks()
                        .iter()
                        .find(|block| block.plain_text() == "After")
                        .map(|block| (block.id(), 0))
                        .or_else(|| {
                            initial
                                .blocks()
                                .iter()
                                .find(|block| block.plain_text() == "Before")
                                .map(|block| (block.id(), "Before".len()))
                        });
                    let selection = if rectangle {
                        Selection::Table(document_core::RectangularSelection {
                            table_id: root.id(),
                            anchor_row: 1,
                            anchor_column: 0,
                            head_row: 0,
                            head_column: 0,
                        })
                    } else {
                        Selection::Text(TextSelection::caret(first_text(root).unwrap()))
                    };
                    document
                        .apply(EditCommand::SetSelection(selection))
                        .unwrap();
                    let before = document.snapshot();
                    document
                        .apply(EditCommand::DeleteBlock { node_id: root.id() })
                        .unwrap();
                    assert_valid(&document);
                    let edited = document.snapshot();
                    let Selection::Text(selection) = edited.selection() else {
                        panic!("caret")
                    };
                    assert!(selection.is_caret());
                    if let Some((id, offset)) = expected {
                        assert_eq!(
                            (selection.anchor.node_id, selection.anchor.text_offset),
                            (id, offset)
                        );
                    } else {
                        assert_eq!(
                            edited.node(selection.anchor.node_id).unwrap().plain_text(),
                            ""
                        );
                    }
                    let saved = edited.serialize().unwrap();
                    assert!(!saved.contains("Ωx"));
                    if prefix.contains("<!--") {
                        assert!(saved.contains("<!-- keep -->"));
                    }
                    document.undo().unwrap();
                    assert_valid(&document);
                    assert_eq!(document.snapshot().serialize().unwrap(), source);
                    assert_eq!(document.snapshot().selection(), before.selection());
                    document.redo().unwrap();
                    assert_valid(&document);
                    assert_eq!(document.snapshot().selection(), edited.selection());
                    document
                        .apply(EditCommand::ReplaceSelection {
                            text: "✓".into(),
                            typing: false,
                        })
                        .unwrap();
                    assert_valid(&document);
                    let typed = document.snapshot().serialize().unwrap();
                    assert!(typed.contains('✓'));
                    if !suffix.is_empty() {
                        assert!(typed.contains("✓After"));
                    } else if prefix.starts_with("Before") {
                        assert!(typed.contains("Before✓"));
                    }
                    document.undo().unwrap();
                    assert_eq!(document.snapshot().serialize().unwrap(), saved);
                }
            }
        }
    }
}

#[test]
fn deleting_a_rectangularly_selected_table_places_caret_at_its_neighbor() {
    let source = "Before\n\n| A | B |\n| --- | --- |\n| C | D |\n\nAfter\n";
    let mut document = Document::from_markdown(source).unwrap();
    let initial = document.snapshot();
    let table_id = initial.blocks().get(1).unwrap().id();
    let next_id = initial.blocks().get(2).unwrap().id();
    document
        .apply(EditCommand::SetSelection(Selection::Table(
            document_core::RectangularSelection {
                table_id,
                anchor_row: 1,
                anchor_column: 1,
                head_row: 0,
                head_column: 0,
            },
        )))
        .unwrap();
    let before = document.snapshot();
    document
        .apply(EditCommand::DeleteBlock { node_id: table_id })
        .unwrap();
    assert_valid(&document);
    let Selection::Text(selection) = document.snapshot().selection().clone() else {
        panic!("text caret")
    };
    assert_eq!(selection.anchor.node_id, next_id);
    assert_eq!(selection.anchor.text_offset, 0);
    assert!(selection.is_caret());
    document.undo().unwrap();
    assert_eq!(document.snapshot().selection(), before.selection());
    assert_eq!(document.snapshot().serialize().unwrap(), source);
    document.redo().unwrap();
    document
        .apply(EditCommand::ReplaceSelection {
            text: "✓".into(),
            typing: false,
        })
        .unwrap();
    assert_valid(&document);
    assert!(document.snapshot().serialize().unwrap().contains("✓After"));
}

#[test]
fn rectangular_endpoints_follow_cell_identity_through_structure_and_history() {
    let source = "| A | B | C |\n| --- | --- | --- |\n| D | E | F |\n| G | H | I |\n";
    for anchor in 0..9 {
        for head in 0..9 {
            for operation in 0..8 {
                for index in 0..3 {
                    let mut document = Document::from_markdown(source).unwrap();
                    let initial = document.snapshot();
                    let BlockNode::Table(table) = initial.blocks().get(0).unwrap().as_ref() else {
                        panic!("table")
                    };
                    let table_id = table.id;
                    let expected_cell = |position: usize| {
                        let (mut row, mut column) = (position / 3, position % 3);
                        // A deleted endpoint chooses its next neighbor on the
                        // deleted axis, or the previous neighbor at the edge.
                        if operation == 1 && row == index {
                            row = if row == 2 { 1 } else { row + 1 };
                        }
                        if operation == 4 && column == index {
                            column = if column == 2 { 1 } else { column + 1 };
                        }
                        table.rows[row].cells[column].id
                    };
                    let expected = [expected_cell(anchor), expected_cell(head)];
                    document
                        .apply(EditCommand::SetSelection(Selection::Table(
                            document_core::RectangularSelection {
                                table_id,
                                anchor_row: anchor / 3,
                                anchor_column: anchor % 3,
                                head_row: head / 3,
                                head_column: head % 3,
                            },
                        )))
                        .unwrap();
                    let before = document.snapshot();
                    let command = match operation {
                        0 => EditCommand::InsertTableRow { table_id, index },
                        1 => EditCommand::DeleteTableRow { table_id, index },
                        2 => EditCommand::MoveTableRow {
                            table_id,
                            from: index,
                            to: (index + 1) % 3,
                        },
                        3 => EditCommand::InsertTableColumn { table_id, index },
                        4 => EditCommand::DeleteTableColumn { table_id, index },
                        5 => EditCommand::MoveTableColumn {
                            table_id,
                            from: index,
                            to: (index + 1) % 3,
                        },
                        6 => EditCommand::DuplicateTableRow { table_id, index },
                        _ => EditCommand::DuplicateTableColumn { table_id, index },
                    };
                    document.apply(command).unwrap();
                    assert_valid(&document);
                    let edited = document.snapshot();
                    let Selection::Table(selection) = edited.selection() else {
                        panic!("rectangle")
                    };
                    let BlockNode::Table(table) = edited.node(table_id).unwrap() else {
                        panic!("table")
                    };
                    assert_eq!(
                        [
                            table.rows[selection.anchor_row].cells[selection.anchor_column].id,
                            table.rows[selection.head_row].cells[selection.head_column].id,
                        ],
                        expected,
                        "anchor={anchor}, head={head}, operation={operation}, index={index}"
                    );
                    let saved = edited.serialize().unwrap();
                    document.undo().unwrap();
                    assert_valid(&document);
                    assert_eq!(document.snapshot().selection(), before.selection());
                    assert_eq!(document.snapshot().serialize().unwrap(), source);
                    document.redo().unwrap();
                    assert_valid(&document);
                    assert_eq!(document.snapshot().selection(), edited.selection());
                    assert_eq!(document.snapshot().serialize().unwrap(), saved);
                }
            }
        }
    }
}

#[test]
fn active_cell_deletion_and_moves_keep_history_and_immediate_typing_valid() {
    let source = "| AΩ | BΩ | CΩ |\n| --- | --- | --- |\n| DΩ | EΩ | FΩ |\n| GΩ | HΩ | IΩ |\n";
    for row in 0..3 {
        for column in 0..3 {
            for operation in 0..5 {
                for affinity in [Affinity::Upstream, Affinity::Downstream] {
                    let mut document = Document::from_markdown(source).unwrap();
                    let initial = document.snapshot();
                    let BlockNode::Table(table) = initial.blocks().get(0).unwrap().as_ref() else {
                        panic!("table")
                    };
                    let table_id = table.id;
                    let active = table.rows[row].cells[column].blocks.get(0).unwrap().id();
                    document
                        .apply(EditCommand::SetSelection(Selection::Text(
                            TextSelection::caret(DocumentPosition::new(active, 3, affinity)),
                        )))
                        .unwrap();
                    let before = document.snapshot();
                    let command = match operation {
                        0 => EditCommand::DeleteTableRow {
                            table_id,
                            index: row,
                        },
                        1 => EditCommand::DeleteTableColumn {
                            table_id,
                            index: column,
                        },
                        2 => EditCommand::MoveTableRow {
                            table_id,
                            from: row,
                            to: (row + 1) % 3,
                        },
                        3 => EditCommand::MoveTableColumn {
                            table_id,
                            from: column,
                            to: (column + 1) % 3,
                        },
                        _ => EditCommand::PasteTsv {
                            table_id,
                            row,
                            column,
                            text: "新\t文\n次\t行".into(),
                        },
                    };
                    document.apply(command).unwrap();
                    assert_valid(&document);
                    let edited = document.snapshot();
                    if matches!(operation, 2 | 3) {
                        assert_eq!(
                            edited.selection(),
                            before.selection(),
                            "moving a cell retains its text identity and affinity"
                        );
                    }
                    let saved = edited.serialize().unwrap();
                    document.undo().unwrap();
                    assert_valid(&document);
                    assert_eq!(document.snapshot().selection(), before.selection());
                    assert_eq!(document.snapshot().serialize().unwrap(), source);
                    document.redo().unwrap();
                    assert_valid(&document);
                    assert_eq!(document.snapshot().selection(), edited.selection());
                    assert_eq!(document.snapshot().serialize().unwrap(), saved);
                    document
                        .apply(EditCommand::ReplaceSelection {
                            text: "✓".into(),
                            typing: false,
                        })
                        .unwrap();
                    assert_valid(&document);
                    assert!(document.snapshot().serialize().unwrap().contains('✓'));
                    document.undo().unwrap();
                    assert_eq!(document.snapshot().serialize().unwrap(), saved);
                }
            }
        }
    }
}

#[test]
fn invalid_table_mutations_leave_content_selection_revision_and_history_unchanged() {
    let source = "| A | B |\n| --- | --- |\n| C | D |\n";
    for operation in 0..4 {
        let mut document = Document::from_markdown(source).unwrap();
        let initial = document.snapshot();
        let table_id = initial.blocks().get(0).unwrap().id();
        document
            .apply(EditCommand::SetTableBorder {
                table_id,
                border: document_core::TableBorder::Dotted,
            })
            .unwrap();
        let before = document.snapshot();
        let command = match operation {
            0 => EditCommand::DeleteTableRow {
                table_id,
                index: usize::MAX,
            },
            1 => EditCommand::DeleteTableColumn {
                table_id,
                index: usize::MAX,
            },
            2 => EditCommand::MoveTableRow {
                table_id,
                from: 0,
                to: usize::MAX,
            },
            _ => EditCommand::MoveTableColumn {
                table_id,
                from: usize::MAX,
                to: 0,
            },
        };
        assert!(document.apply(command).is_err());
        let after = document.snapshot();
        assert_eq!(after.revision(), before.revision());
        assert_eq!(after.selection(), before.selection());
        assert_eq!(after.serialize().unwrap(), before.serialize().unwrap());
        assert_valid(&document);
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        document.redo().unwrap();
        assert_eq!(
            document.snapshot().serialize().unwrap(),
            before.serialize().unwrap()
        );
    }
}
