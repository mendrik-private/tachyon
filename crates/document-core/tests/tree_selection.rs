use document_core::{
    Affinity, BlockNode, DefinitionKind, Document, DocumentPosition, EditCommand, RichClipboard,
    Selection, TextSelection,
};

fn leaves(block: &BlockNode, output: &mut Vec<(document_core::NodeId, String)>) {
    if let Some(text) = block.text() {
        output.push((block.id(), text.as_string()));
    }
    match block {
        BlockNode::Definition { blocks, .. }
        | BlockNode::BlockQuote { blocks, .. }
        | BlockNode::Alert { blocks, .. }
        | BlockNode::FootnoteDefinition { blocks, .. } => {
            for b in blocks {
                leaves(b, output);
            }
        }
        BlockNode::List(list) => {
            for item in list.items.iter() {
                for b in &item.blocks {
                    leaves(b, output);
                }
            }
        }
        BlockNode::Table(table) => {
            for row in table.rows.iter() {
                for cell in row.cells.iter() {
                    for b in &cell.blocks {
                        leaves(b, output);
                    }
                }
            }
        }
        _ => {}
    }
}

fn positions(document: &Document) -> Vec<(document_core::NodeId, String)> {
    let mut output = Vec::new();
    for b in document.snapshot().blocks() {
        leaves(b, &mut output);
    }
    output
}

fn select(
    document: &mut Document,
    start: usize,
    from: usize,
    end: usize,
    to: usize,
    reverse: bool,
) {
    let nodes = positions(document);
    let a = DocumentPosition::new(nodes[start].0, from, Affinity::Downstream);
    let b = DocumentPosition::new(nodes[end].0, to, Affinity::Upstream);
    document
        .apply(EditCommand::SetSelection(Selection::Text(TextSelection {
            anchor: if reverse { b } else { a },
            head: if reverse { a } else { b },
        })))
        .unwrap();
}

#[test]
fn cross_definition_copy_preserves_relationships_and_rich_text() {
    let source = "# Untouched\n\nCache\n: Reusable **geometry**.\n: A second explanation.\n\nViewport\n: A *visible* region.\n\nTail\n";
    for reverse in [false, true] {
        let mut document = Document::from_markdown(source).unwrap();
        select(&mut document, 1, 2, 5, 9, reverse);
        let before = document.snapshot();
        let payload = before.clipboard_payload().unwrap().unwrap();
        assert_eq!(
            payload.plain_text.as_deref(),
            Some("che\nReusable geometry.\nA second explanation.\nViewport\nA visible")
        );
        let rich = RichClipboard::from_json(payload.rich_json.as_deref().unwrap()).unwrap();
        assert!(rich.markdown.contains("**geometry**"));
        let copied = Document::from_markdown(rich.markdown.as_str()).unwrap();
        assert!(matches!(
            copied.snapshot().blocks().get(0).unwrap().as_ref(),
            BlockNode::Definition {
                kind: DefinitionKind::List,
                ..
            }
        ));
        assert_eq!(
            positions(&copied)
                .iter()
                .map(|(_, t)| t.as_str())
                .collect::<Vec<_>>(),
            [
                "che",
                "Reusable geometry.",
                "A second explanation.",
                "Viewport",
                "A visible"
            ]
        );
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        assert_eq!(document.snapshot().revision(), before.revision());
    }
}

#[test]
fn cross_definition_replace_keeps_unselected_text_in_its_authored_roles() {
    let source = "# Untouched\n\nCache\n: Reusable **geometry**.\n: A second explanation.\n\nViewport\n: A *visible* region.\n\nTail\n";
    for reverse in [false, true] {
        let mut document = Document::from_markdown(source).unwrap();
        select(&mut document, 1, 2, 2, 9, reverse);
        let before = document.snapshot();
        document
            .apply(EditCommand::ReplaceSelection {
                text: "x".into(),
                typing: false,
            })
            .unwrap();
        assert_eq!(
            positions(&document)
                .iter()
                .map(|(_, t)| t.as_str())
                .collect::<Vec<_>>(),
            [
                "Untouched",
                "Cax",
                "geometry.",
                "A second explanation.",
                "Viewport",
                "A visible region.",
                "Tail"
            ]
        );
        let after = document.snapshot();
        let serialized = after.serialize().unwrap();
        assert!(serialized.starts_with("# Untouched\n\n"));
        assert!(serialized.ends_with("\n\nTail\n"), "{serialized:?}");
        let reopened = Document::from_markdown(serialized.as_str()).unwrap();
        assert_eq!(
            positions(&reopened)
                .iter()
                .map(|(_, t)| t)
                .collect::<Vec<_>>(),
            positions(&document)
                .iter()
                .map(|(_, t)| t)
                .collect::<Vec<_>>()
        );
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        assert_eq!(document.snapshot().selection(), before.selection());
        document.redo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), serialized);
    }
}

fn texts(document: &Document) -> Vec<String> {
    positions(document)
        .into_iter()
        .map(|(_, text)| text)
        .collect()
}

#[test]
fn nested_sibling_paragraphs_join_but_separate_items_do_not() {
    for reverse in [false, true] {
        let mut document = Document::from_markdown(
            "> - First **paragraph**.\n>\n>   Second *paragraph*.\n>\n> - Another item.\n\nTail\n",
        )
        .unwrap();
        select(&mut document, 0, 6, 1, 7, reverse);
        document
            .apply(EditCommand::ReplaceSelection {
                text: "new ".into(),
                typing: false,
            })
            .unwrap();
        assert_eq!(
            texts(&document),
            ["First new paragraph.", "Another item.", "Tail"]
        );
        let saved = document.snapshot().serialize().unwrap();
        assert!(saved.contains("*paragraph*"), "{saved}");
        assert_eq!(
            texts(&Document::from_markdown(saved.as_str()).unwrap()),
            texts(&document)
        );
        document.undo().unwrap();

        select(&mut document, 0, 6, 2, 8, reverse);
        document
            .apply(EditCommand::ReplaceSelection {
                text: "new".into(),
                typing: false,
            })
            .unwrap();
        assert_eq!(texts(&document), ["First new", "item.", "Tail"]);
        let saved = document.snapshot().serialize().unwrap();
        assert_eq!(
            texts(&Document::from_markdown(saved.as_str()).unwrap()),
            texts(&document)
        );
    }
}

#[test]
fn copied_ordered_items_keep_authored_start_and_tasks_keep_state() {
    let mut document =
        Document::from_markdown("7. First\n8. Second\n9. Third\n\n- [x] Done\n- [ ] Pending\n")
            .unwrap();
    select(&mut document, 1, 0, 2, 5, false);
    let payload = document.snapshot().clipboard_payload().unwrap().unwrap();
    let rich = RichClipboard::from_json(payload.rich_json.as_deref().unwrap()).unwrap();
    let copied = Document::from_markdown(rich.markdown.as_str()).unwrap();
    let snapshot = copied.snapshot();
    let BlockNode::List(list) = snapshot.blocks().get(0).unwrap().as_ref() else {
        panic!("list lost");
    };
    assert_eq!(list.kind, document_core::ListKind::Ordered { start: 8 });
    assert_eq!(texts(&copied), ["Second", "Third"]);

    select(&mut document, 3, 1, 4, 4, true);
    let payload = document.snapshot().clipboard_payload().unwrap().unwrap();
    let rich = RichClipboard::from_json(payload.rich_json.as_deref().unwrap()).unwrap();
    let copied = Document::from_markdown(rich.markdown.as_str()).unwrap();
    let snapshot = copied.snapshot();
    let BlockNode::List(list) = snapshot.blocks().get(0).unwrap().as_ref() else {
        panic!("tasks lost");
    };
    assert_eq!(
        list.items
            .iter()
            .map(|item| item.checked)
            .collect::<Vec<_>>(),
        [Some(true), Some(false)]
    );
    assert_eq!(texts(&copied), ["one", "Pend"]);
}

#[test]
fn cross_cell_edit_clears_values_without_deleting_cells_or_headers() {
    let source = "| Name | Count |\n| :--- | ---: |\n| Alpha | **120** |\n| Beta | 250 |\n\nTail\n";
    for reverse in [false, true] {
        let mut document = Document::from_markdown(source).unwrap();
        let original = document.snapshot();
        let BlockNode::Table(original_table) = original.blocks().get(0).unwrap().as_ref() else {
            panic!("table");
        };
        select(&mut document, 2, 2, 5, 1, reverse);
        document
            .apply(EditCommand::ReplaceSelection {
                text: "x".into(),
                typing: false,
            })
            .unwrap();
        assert_eq!(
            texts(&document),
            ["Name", "Count", "Alx", "", "", "50", "Tail"]
        );
        let snapshot = document.snapshot();
        let BlockNode::Table(table) = snapshot.blocks().get(0).unwrap().as_ref() else {
            panic!("table lost");
        };
        assert_eq!(table.header_rows, original_table.header_rows);
        assert_eq!(table.columns.as_ref(), original_table.columns.as_ref());
        assert_eq!(
            table.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
            original_table
                .rows
                .iter()
                .map(|row| row.id)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            table
                .rows
                .iter()
                .flat_map(|row| row.cells.iter().map(|cell| cell.id))
                .collect::<Vec<_>>(),
            original_table
                .rows
                .iter()
                .flat_map(|row| row.cells.iter().map(|cell| cell.id))
                .collect::<Vec<_>>()
        );
        assert!(
            table
                .rows
                .iter()
                .all(|row| row.cells.iter().all(|cell| !cell.blocks.is_empty()))
        );
        assert_eq!(
            texts(&Document::from_markdown(snapshot.serialize().unwrap().as_str()).unwrap()),
            texts(&document)
        );
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }
}

#[test]
fn partial_table_copy_keeps_columns_without_copying_unselected_values() {
    let mut document = Document::from_markdown(
        "| Name | Count |\n| :--- | ---: |\n| Alpha | **120** |\n| Beta | 250 |\n",
    )
    .unwrap();
    select(&mut document, 3, 1, 4, 2, false);
    let payload = document.snapshot().clipboard_payload().unwrap().unwrap();
    let rich = RichClipboard::from_json(payload.rich_json.as_deref().unwrap()).unwrap();
    assert!(!rich.markdown.contains("Alpha"));
    assert!(!rich.markdown.contains("250"));
    assert!(!rich.markdown.contains("Name"));
    let copied = Document::from_markdown(rich.markdown.as_str()).unwrap();
    let snapshot = copied.snapshot();
    let BlockNode::Table(table) = snapshot.blocks().get(0).unwrap().as_ref() else {
        panic!("table lost: {}", rich.markdown);
    };
    assert_eq!(table.header_rows, 0);
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.columns.len(), 2);
    assert_eq!(
        texts(&copied)
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>(),
        ["20", "Be"]
    );
}

#[test]
fn rich_and_block_pastes_stay_within_the_target_definition_role() {
    let source = "Cache\n: Reusable **geometry**.\n: Another description.\n\nTail\n";
    for reverse in [false, true] {
        let mut document = Document::from_markdown(source).unwrap();
        select(&mut document, 0, 2, 1, 9, reverse);
        document
            .apply(EditCommand::PasteMarkdown {
                markdown: "*fresh*".into(),
            })
            .unwrap();
        assert_eq!(
            texts(&document),
            ["Cafresh", "geometry.", "Another description.", "Tail"]
        );
        assert!(document.snapshot().serialize().unwrap().contains("*fresh*"));
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);

        select(&mut document, 1, 9, 2, 8, reverse);
        document
            .apply(EditCommand::PasteMarkdown {
                markdown: "## Example\n\n- One\n- Two\n".into(),
            })
            .unwrap();
        assert_eq!(
            texts(&document),
            [
                "Cache",
                "Reusable ",
                "Example",
                "One",
                "Two",
                "description.",
                "Tail"
            ]
        );
        let snapshot = document.snapshot();
        let BlockNode::Definition { blocks, .. } = snapshot.blocks().get(0).unwrap().as_ref()
        else {
            panic!("definition lost");
        };
        let BlockNode::Definition { kind, blocks, .. } = blocks.get(1).unwrap().as_ref() else {
            panic!("description lost");
        };
        assert_eq!(*kind, DefinitionKind::Description);
        assert!(matches!(
            blocks.get(1).unwrap().as_ref(),
            BlockNode::Heading(_)
        ));
        let serialized = snapshot.serialize().unwrap();
        assert_eq!(
            texts(&Document::from_markdown(serialized.as_str()).unwrap()),
            texts(&document)
        );
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        document.redo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), serialized);
    }
}

#[test]
fn block_paste_splits_one_nested_paragraph_with_unique_ids() {
    let mut document = Document::from_markdown("Term\n: Before after.\n").unwrap();
    select(&mut document, 1, 7, 1, 7, false);
    document
        .apply(EditCommand::PasteMarkdown {
            markdown: "```rust\nx();\n```\n".into(),
        })
        .unwrap();
    assert_eq!(texts(&document), ["Term", "Before ", "x();\n", "after."]);
    let ids = positions(&document)
        .into_iter()
        .map(|(id, _)| id)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids.len(), 4);
    assert_eq!(
        texts(&Document::from_markdown(document.snapshot().serialize().unwrap().as_str()).unwrap()),
        texts(&document)
    );
}

#[test]
fn composition_across_roles_is_provisional_reversible_and_unicode_safe() {
    let source = "Café\n: Visible **geometry**.\n\nTail\n";
    let mut document = Document::from_markdown(source).unwrap();
    select(&mut document, 0, 3, 1, 8, true);
    let before = document.snapshot();
    let Selection::Text(selection) = before.selection().clone() else {
        panic!("text");
    };
    document.begin_composition(selection.clone()).unwrap();
    document.update_composition("é".into()).unwrap();
    assert_eq!(texts(&document), ["Café", "geometry.", "Tail"]);
    document.update_composition("界".into()).unwrap();
    assert_eq!(texts(&document), ["Caf界", "geometry.", "Tail"]);
    document.cancel_composition().unwrap();
    assert_eq!(document.snapshot().serialize().unwrap(), source);
    assert_eq!(document.snapshot().selection(), before.selection());
    document.begin_composition(selection.clone()).unwrap();
    document.update_composition("界".into()).unwrap();
    document.commit_composition().unwrap();
    document.undo().unwrap();
    assert_eq!(document.snapshot().serialize().unwrap(), source);

    let mut invalid = selection;
    invalid.head.text_offset = 4; // Inside the UTF-8 encoding of é.
    let before = document.snapshot();
    assert!(document.begin_composition(invalid).is_err());
    assert_eq!(document.snapshot().revision(), before.revision());
    assert_eq!(document.snapshot().selection(), before.selection());
    assert_eq!(document.snapshot().serialize().unwrap(), source);
}

#[test]
fn every_mixed_container_range_copies_edits_reopens_and_undoes() {
    let source = concat!(
        "# Opening\n\nBefore\n\n",
        "Term\n: **Description**\n\n",
        "> Quotation\n>\n> - Inner\n> - Other\n\n",
        "7. Ordered\n8. Sequence\n\n",
        "| Name | Count |\n| --- | ---: |\n| Alpha | 120 |\n| Beta | 250 |\n\n",
        "After\n"
    );
    let mut document = Document::from_markdown(source).unwrap();
    let original = texts(&document);
    for first in 0..original.len() {
        for last in first + 1..original.len() {
            for reverse in [false, true] {
                select(&mut document, first, 1, last, 2, reverse);
                let payload = document.snapshot().clipboard_payload().unwrap().unwrap();
                let rich = RichClipboard::from_json(payload.rich_json.as_deref().unwrap()).unwrap();
                let reopened = Document::from_markdown(rich.markdown.as_str()).unwrap();
                let mut expected = original[first..=last].to_vec();
                expected[0] = original[first][1..].to_owned();
                *expected.last_mut().unwrap() = original[last][..2].to_owned();
                assert_eq!(
                    texts(&reopened)
                        .into_iter()
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>(),
                    expected,
                    "range {first}..{last}, reverse={reverse}, fragment={}",
                    rich.markdown
                );
                assert_eq!(document.snapshot().serialize().unwrap(), source);
                let before = document.snapshot();
                document
                    .apply(EditCommand::ReplaceSelection {
                        text: "x".into(),
                        typing: false,
                    })
                    .unwrap();
                let edited = document.snapshot().serialize().unwrap();
                let reopened = Document::from_markdown(edited.as_str()).unwrap();
                assert_eq!(
                    texts(&reopened)
                        .into_iter()
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>(),
                    texts(&document)
                        .into_iter()
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>(),
                    "edited range {first}..{last}, reverse={reverse}, saved={edited}"
                );
                document.undo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), source);
                assert_eq!(document.snapshot().selection(), before.selection());
            }
        }
    }
}

#[test]
fn selected_opaque_blocks_survive_copy_and_untouched_subtrees_are_shared() {
    let source = "# Opening\n\n> Before\n\n<custom-widget>literal evidence</custom-widget>\n\n---\n\nTerm\n: After\n\nTail\n";
    let mut document = Document::from_markdown(source).unwrap();
    let end = texts(&document)
        .iter()
        .position(|text| text == "After")
        .unwrap();
    select(&mut document, 1, 2, end, 2, false);
    let before = document.snapshot();
    let payload = before.clipboard_payload().unwrap().unwrap();
    let rich = RichClipboard::from_json(payload.rich_json.as_deref().unwrap()).unwrap();
    assert!(
        rich.markdown
            .contains("<custom-widget>literal evidence</custom-widget>")
    );
    assert!(rich.markdown.contains("---"));
    document
        .apply(EditCommand::ReplaceSelection {
            text: "x".into(),
            typing: false,
        })
        .unwrap();
    assert_eq!(texts(&document), ["Opening", "Bex", "ter", "Tail"]);
    let after = document.snapshot();
    assert!(std::sync::Arc::ptr_eq(
        before.blocks().get(0).unwrap(),
        after.blocks().get(0).unwrap()
    ));
    assert!(std::sync::Arc::ptr_eq(
        before.blocks().get(before.blocks().len() - 1).unwrap(),
        after.blocks().get(after.blocks().len() - 1).unwrap()
    ));
    assert!(!after.serialize().unwrap().contains("custom-widget"));
    document.undo().unwrap();
    assert_eq!(document.snapshot().serialize().unwrap(), source);
}
