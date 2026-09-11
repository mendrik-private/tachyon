use document_core::{
    Affinity, BlockNode, Document, DocumentPosition, EditCommand, NodeId, Selection, TextSelection,
};

fn leaf(block: &BlockNode, text: &str) -> Option<NodeId> {
    if block.text().is_some_and(|t| t.as_string() == text) {
        return Some(block.id());
    }
    match block {
        BlockNode::List(list) => list
            .items
            .iter()
            .flat_map(|i| i.blocks.iter())
            .find_map(|b| leaf(b, text)),
        BlockNode::Table(table) => table
            .rows
            .iter()
            .flat_map(|row| row.cells.iter())
            .flat_map(|cell| cell.blocks.iter())
            .find_map(|b| leaf(b, text)),
        BlockNode::BlockQuote { blocks, .. }
        | BlockNode::Alert { blocks, .. }
        | BlockNode::Definition { blocks, .. }
        | BlockNode::FootnoteDefinition { blocks, .. } => blocks.iter().find_map(|b| leaf(b, text)),
        _ => None,
    }
}

fn edit(document: &mut Document, text: &str, inserted: &str) {
    let id = document
        .snapshot()
        .blocks()
        .iter()
        .find_map(|b| leaf(b, text))
        .unwrap();
    document
        .apply(EditCommand::ReplaceText {
            node_id: id,
            range: 0..0,
            text: inserted.into(),
            selection_after: None,
            typing: false,
        })
        .unwrap();
}

#[test]
fn unlabeled_list_items_have_own_caret_hosts_and_source_identity() {
    for source in ["-\n", "-\n  - Child-text.\n", "7.\n   1. Child-text.\n"] {
        let mut document = Document::from_markdown(source).unwrap();
        let snapshot = document.snapshot();
        let BlockNode::List(list) = snapshot.blocks().get(0).unwrap().as_ref() else {
            panic!("list");
        };
        let item = &list.items[0];
        let Some(BlockNode::Paragraph(host)) = item.blocks.get(0).map(AsRef::as_ref) else {
            panic!("unlabeled item needs its own blank caret row, before any child list");
        };
        assert!(host.content.is_empty());
        let host_id = host.id;
        assert_eq!(snapshot.serialize().unwrap(), source);
        if source.contains("Child-text.") {
            edit(&mut document, "Child-text.", "x");
            assert_eq!(
                document.snapshot().serialize().unwrap(),
                source.replace("Child-text.", "xChild\\-text\\.")
            );
            document.undo().unwrap();
        }
        document
            .apply(EditCommand::ReplaceText {
                node_id: host_id,
                range: 0..0,
                text: "Introduction".into(),
                selection_after: None,
                typing: false,
            })
            .unwrap();
        let saved = document.snapshot().serialize().unwrap();
        if source.contains("Child-text.") {
            assert!(
                saved.contains("Child-text."),
                "typing in a blank parent must preserve untouched child spelling: {saved}"
            );
        }
        assert!(saved.starts_with(if source.starts_with('7') {
            "7. Introduction"
        } else {
            "- Introduction"
        }));
        assert_reopens(&document, &saved);
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }
}

#[test]
fn blank_parent_insertion_preserves_marker_context_and_descendant_bytes() {
    for marker in ["-", "+", "*", "7.", "7)"] {
        for newline in ["\n", "\r\n"] {
            for quoted in [false, true] {
                for padding in ["", "  "] {
                    let rail = if quoted { "> " } else { "" };
                    let indent = " ".repeat(marker.len() + padding.len().max(1));
                    let source = format!(
                        "{rail}{marker}{padding}{newline}{rail}{indent}- Child-text.{newline}"
                    );
                    let mut document = Document::from_markdown(source.as_str()).unwrap();
                    assert_eq!(document.snapshot().serialize().unwrap(), source);
                    edit(&mut document, "", "Introduction");
                    let space = if padding.is_empty() { " " } else { padding };
                    let expected = format!(
                        "{rail}{marker}{space}Introduction{newline}{rail}{indent}- Child-text.{newline}"
                    );
                    let saved = document.snapshot().serialize().unwrap();
                    assert_eq!(saved, expected);
                    assert_reopens(&document, &saved);
                    document.undo().unwrap();
                    assert_eq!(document.snapshot().serialize().unwrap(), source);
                    document.redo().unwrap();
                    assert_eq!(document.snapshot().serialize().unwrap(), expected);
                }
            }
        }
    }
}

#[test]
fn rich_html_cell_text_edit_retains_authored_table_bytes() {
    for (source, caption) in [
        (
            "<table><tr><td><p>Caption</p></td></tr></table>\n",
            "Caption",
        ),
        (
            include_str!("../../../performance/layout-fixtures/104-cramped-image-states.md"),
            "Caption inside the narrow image cell.",
        ),
    ] {
        let mut document = Document::from_markdown(source).unwrap();
        let original = document.snapshot();
        edit(&mut document, caption, "x");
        let edited = document.snapshot();
        for block in original.blocks() {
            let BlockNode::Table(before) = block.as_ref() else {
                continue;
            };
            let BlockNode::Table(after) = edited.node(before.id).unwrap() else {
                panic!("table identity");
            };
            assert_eq!(before.columns, after.columns);
            assert_eq!(before.header_rows, after.header_rows);
            assert_eq!(before.border, after.border);
            assert_eq!(before.preserved_metadata, after.preserved_metadata);
            for (old_row, new_row) in before.rows.iter().zip(after.rows.iter()) {
                assert_eq!(old_row.id, new_row.id);
                for (old_cell, new_cell) in old_row.cells.iter().zip(new_row.cells.iter()) {
                    assert_eq!(old_cell.id, new_cell.id);
                    for (old, new) in old_cell.blocks.iter().zip(new_cell.blocks.iter()) {
                        assert_eq!(old.id(), new.id());
                        if leaf(old, caption).is_none() {
                            assert!(std::sync::Arc::ptr_eq(old, new));
                        }
                    }
                }
            }
        }
        let saved = edited.serialize().unwrap();
        assert_eq!(saved, source.replacen(caption, &format!("x{caption}"), 1));
        assert_reopens(&document, &saved);
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }
}

#[test]
fn html_cell_addresses_preserve_quotes_entities_comments_and_inline_markup() {
    for (body, text, expected) in [
        ("", "", "x"),
        (
            "<p><b title='<!--fake-->'>Caption</b></p>",
            "Caption",
            "<p><b title='<!--fake-->'>xCaption</b></p>",
        ),
        (
            "<p data-label='a > <p>fake</p>'><b>Caption</b></p>",
            "Caption",
            "<p data-label='a > <p>fake</p>'><b>xCaption</b></p>",
        ),
        (
            "<!-- <p>Caption</p> --><blockquote><p>Caption</p></blockquote>",
            "Caption",
            "<!-- <p>Caption</p> --><blockquote><p>xCaption</p></blockquote>",
        ),
        (
            "<p>A &amp; B &#233; &copy; &NotEqualTilde;</p>",
            "A & B é © ≂̸",
            "<p>xA &amp; B &#233; &copy; &NotEqualTilde;</p>",
        ),
        (
            "<p><a title='same' href='/local'>東京</a></p>",
            "東京",
            "<p><a title='same' href='/local'>x東京</a></p>",
        ),
    ] {
        for newline in ["\n", "\r\n"] {
            let source = format!(
                "<TABLE data-table='keep'>{newline}<TBODY><TR><TD>{body}</TD><TD><p>Neighbor</p></TD></TR></TBODY>{newline}</TABLE>{newline}"
            );
            let mut document = Document::from_markdown(source.as_str()).unwrap();
            edit(&mut document, text, "x");
            let saved = document.snapshot().serialize().unwrap();
            assert_eq!(
                saved,
                source.replacen(
                    &format!("<TD>{body}</TD>"),
                    &format!("<TD>{expected}</TD>"),
                    1
                )
            );
            assert_reopens(&document, &saved);
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        }
    }
}

#[test]
fn html_cell_source_addresses_do_not_confuse_repeated_text() {
    let source = "<table><thead><tr><th>Same</th><th>Same</th></tr></thead><tbody><tr><td><p>Same</p></td><td><blockquote><p>Same</p></blockquote></td></tr></tbody></table>\n";
    let mut document = Document::from_markdown(source).unwrap();
    let snapshot = document.snapshot();
    let BlockNode::Table(table) = snapshot.blocks().get(0).unwrap().as_ref() else {
        panic!("table");
    };
    let id = table.rows[1].cells[1]
        .blocks
        .iter()
        .find_map(|block| leaf(block, "Same"))
        .unwrap();
    for (range, insertion, expected) in [
        (4..4, "!", "Same!"),
        (0..1, "s", "same!"),
        (4..5, "", "same"),
    ] {
        document
            .apply(EditCommand::ReplaceText {
                node_id: id,
                range,
                text: insertion.into(),
                selection_after: None,
                typing: false,
            })
            .unwrap();
        let saved = document.snapshot().serialize().unwrap();
        assert_eq!(
            saved,
            source.replace(
                "<blockquote><p>Same</p>",
                &format!("<blockquote><p>{expected}</p>")
            )
        );
        assert_reopens(&document, &saved);
    }
    for _ in 0..3 {
        document.undo().unwrap();
    }
    assert_eq!(document.snapshot().serialize().unwrap(), source);
}

#[test]
fn html_cell_formatting_preserves_the_authored_outer_markup() {
    let source = "<table data-owner='keep'><tbody><tr><td><p class='caption'>Caption</p></td><td><p>Neighbor</p></td></tr></tbody></table>\n";
    let mut document = Document::from_markdown(source).unwrap();
    let id = document
        .snapshot()
        .blocks()
        .iter()
        .find_map(|b| leaf(b, "Caption"))
        .unwrap();
    document
        .apply(EditCommand::ToggleInline {
            node_id: id,
            range: 0..7,
            format: document_core::InlineFormat::Bold,
        })
        .unwrap();
    let saved = document.snapshot().serialize().unwrap();
    assert_eq!(
        saved,
        source.replace(">Caption</p>", "><strong>Caption</strong></p>")
    );
    assert_reopens(&document, &saved);
    document.undo().unwrap();
    assert_eq!(document.snapshot().serialize().unwrap(), source);
}

fn shape(block: &BlockNode) -> String {
    let sequence =
        |blocks: &document_core::BlockSequence| blocks.iter().map(|b| shape(b)).collect::<Vec<_>>();
    match block {
        BlockNode::Table(table) => format!(
            "table:{:?}:{}:{:?}",
            table.columns,
            table.header_rows,
            table
                .rows
                .iter()
                .map(|r| r
                    .cells
                    .iter()
                    .map(|c| sequence(&c.blocks))
                    .collect::<Vec<_>>())
                .collect::<Vec<_>>()
        ),
        BlockNode::List(list) => format!(
            "{:?}:{}:{:?}",
            list.kind,
            list.tight,
            list.items
                .iter()
                .map(|i| (i.checked, sequence(&i.blocks)))
                .collect::<Vec<_>>()
        ),
        BlockNode::BlockQuote { blocks, .. } => format!("quote:{:?}", sequence(blocks)),
        BlockNode::Alert { kind, blocks, .. } => format!("{kind:?}:{:?}", sequence(blocks)),
        BlockNode::Definition { kind, blocks, .. } => format!("{kind:?}:{:?}", sequence(blocks)),
        BlockNode::FootnoteDefinition { label, blocks, .. } => {
            format!("{label}:{:?}", sequence(blocks))
        }
        _ => format!("{:?}:{}", std::mem::discriminant(block), block.plain_text()),
    }
}

fn assert_reopens(document: &Document, saved: &str) {
    let reopened = Document::from_markdown(saved).unwrap();
    let shapes = |doc: &Document| {
        doc.snapshot()
            .blocks()
            .iter()
            .map(|b| shape(b))
            .collect::<Vec<_>>()
    };
    assert_eq!(shapes(document), shapes(&reopened), "reopen: {saved}");
}

fn check(source: &str, expected: &str) {
    let mut document = Document::from_markdown(source).unwrap();
    edit(&mut document, "a", "x");
    let saved = document.snapshot().serialize().unwrap();
    assert_eq!(saved, expected);
    assert_reopens(&document, &saved);
    document.undo().unwrap();
    assert_eq!(document.snapshot().serialize().unwrap(), source);
    document.redo().unwrap();
    assert_eq!(document.snapshot().serialize().unwrap(), expected);
}

#[test]
fn editing_code_parent_keeps_unmodified_child_source() {
    let source = "- ```rust\n  a\n  ```\n  - Code-led explanation\n";
    let mut document = Document::from_markdown(source).unwrap();
    edit(&mut document, "a\n", "x");
    let saved = document.snapshot().serialize().unwrap();
    assert_eq!(saved, source.replace("  a\n", "  xa\n"));
    assert_reopens(&document, &saved);
    document.undo().unwrap();
    assert_eq!(document.snapshot().serialize().unwrap(), source);
}

#[test]
fn editing_text_parent_keeps_unmodified_child_source() {
    for (source, expected) in [
        (
            "- a\n  - Child-led explanation\n",
            "- xa\n  - Child-led explanation\n",
        ),
        (
            "- ### a\n  - Child-led explanation\n",
            "- ### xa\n  - Child-led explanation\n",
        ),
        (
            "- ![a](image.svg)\n  - Child-led explanation\n",
            "- ![xa](image.svg)\n  - Child-led explanation\n",
        ),
    ] {
        check(source, expected);
    }
}

#[test]
fn text_parent_edits_preserve_descendants_across_source_contexts() {
    for parent in [
        "a",
        "### a",
        "![a](image.svg)",
        "[![a](image.svg \"Caption\")](https://example.test \"Link\")",
    ] {
        for (prefix, indent) in [("- ", "  "), ("+ ", "  "), ("003) ", "     ")] {
            let base = format!(
                "{prefix}{parent}\n{indent}- Child-led: keep [reference][r] &amp; **bold**\n\n[r]: /local \"Authored title\"\n"
            );
            for source in [
                base.clone(),
                base.lines()
                    .map(|line| format!("> {line}\n"))
                    .collect::<String>(),
                format!(
                    "> [!NOTE]\n{}",
                    base.lines()
                        .map(|line| format!("> {line}\n"))
                        .collect::<String>()
                ),
            ] {
                for newline in ["\n", "\r\n"] {
                    let source = source.replace('\n', newline);
                    let mut document = Document::from_markdown(source.as_str()).unwrap();
                    let child = document
                        .snapshot()
                        .blocks()
                        .iter()
                        .find_map(|block| leaf(block, "Child-led: keep reference & bold"))
                        .unwrap();
                    edit(&mut document, "a", "x");
                    let saved = document.snapshot().serialize().unwrap();
                    let changed_parent = if parent.contains("[a]") {
                        parent.replace("[a]", "[xa]")
                    } else {
                        parent.replacen('a', "xa", 1)
                    };
                    assert_eq!(saved, source.replacen(parent, &changed_parent, 1));
                    assert!(!document.snapshot().dirty_node_ids().contains(&child));
                    assert_reopens(&document, &saved);
                    document.undo().unwrap();
                    assert_eq!(document.snapshot().serialize().unwrap(), source);
                    document.redo().unwrap();
                    assert_eq!(document.snapshot().serialize().unwrap(), saved);
                }
            }
        }
    }
    for source in [
        "- [ ] a\n  - Child-led explanation\n",
        "- [X] a\n  - Child-led explanation\n",
        "- Outer\n  - a\n    - Child-led explanation\n",
    ] {
        check(source, &source.replacen(" a\n", " xa\n", 1));
    }
}

#[test]
fn parent_leaf_escaping_and_task_continuations_do_not_rewrite_children() {
    for source in [
        "- a\n  - Child-led explanation\n",
        "- ### a\n  - Child-led explanation\n",
        "- ![a](image.svg)\n  - Child-led explanation\n",
    ] {
        for inserted in ["*", "[", "2. ", "é ", "東京 "] {
            let mut document = Document::from_markdown(source).unwrap();
            edit(&mut document, "a", inserted);
            let saved = document.snapshot().serialize().unwrap();
            assert!(saved.contains("  - Child-led explanation\n"));
            assert_reopens(&document, &saved);
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        }
    }
    for source in [
        "- [ ] a\n  - Child-led explanation\n",
        "> - [X] a\n>   - Child-led explanation\n",
    ] {
        let mut document = Document::from_markdown(source).unwrap();
        edit(&mut document, "a", "first  \n");
        let saved = document.snapshot().serialize().unwrap();
        assert!(saved.contains("- Child-led explanation\n"));
        assert_reopens(&document, &saved);
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }
}

#[test]
fn task_parent_toggle_and_typing_share_source_local_patches() {
    let source = "- [ ] a\n  - Child-led explanation\n";
    for toggle_first in [true, false] {
        let mut document = Document::from_markdown(source).unwrap();
        let BlockNode::List(list) = document
            .snapshot()
            .blocks()
            .get(0)
            .unwrap()
            .as_ref()
            .clone()
        else {
            panic!("list");
        };
        let item_id = list.items[0].id;
        if toggle_first {
            document.apply(EditCommand::ToggleTask { item_id }).unwrap();
        }
        edit(&mut document, "a", "x");
        if !toggle_first {
            document.apply(EditCommand::ToggleTask { item_id }).unwrap();
        }
        let saved = document.snapshot().serialize().unwrap();
        assert_eq!(saved, "- [x] xa\n  - Child-led explanation\n");
        assert_reopens(&document, &saved);
        document.undo().unwrap();
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }
}

#[test]
fn image_parent_attributes_preserve_child_source_and_enclosing_link() {
    let source = "- [![a](old.svg \"Caption\")](https://example.test \"Destination\")\n  - Child-led explanation\n";
    let mut document = Document::from_markdown(source).unwrap();
    let image_id = document
        .snapshot()
        .blocks()
        .iter()
        .find_map(|block| leaf(block, "a"))
        .unwrap();
    document
        .apply(EditCommand::SetImageAttributes {
            image_id,
            source: "new.svg".into(),
            alt: "Updated alt".into(),
        })
        .unwrap();
    let saved = document.snapshot().serialize().unwrap();
    assert_eq!(
        saved,
        source.replace("![a](old.svg", "![Updated alt](new.svg")
    );
    assert_reopens(&document, &saved);
    document.undo().unwrap();
    assert_eq!(document.snapshot().serialize().unwrap(), source);
}

#[test]
fn fenced_code_edits_preserve_children_across_markers_quotes_and_line_endings() {
    for source in [
        "- ```rust\n  a\n  ```\n  - Code-led explanation\n  + Keep: [reference](/local) &amp; text\n",
        "007) ```rust\n     a\n     ```\n     - Code-led explanation\n",
        "> - ```rust\n>   a\n>   ```\n>   - Code-led explanation\n",
        "> [!NOTE]\n> - ```rust\n>   a\n>   ```\n>   - Code-led explanation\n",
        "- Parent\n  - ```rust\n    a\n    ```\n    - Code-led explanation\n",
        "- ```rust\n  a\n  ```\n  - Code-led explanation",
    ] {
        for newline in ["\n", "\r\n"] {
            let source = source.replace('\n', newline);
            for inserted in ["x", "```\n", "東京\n", "\n"] {
                let mut document = Document::from_markdown(source.as_str()).unwrap();
                edit(&mut document, &format!("a{newline}"), inserted);
                let saved = document.snapshot().serialize().unwrap();
                assert!(saved.contains("- Code-led explanation"), "{saved}");
                if inserted == "x" {
                    assert_eq!(
                        saved,
                        source.replace(&format!("a{newline}"), &format!("xa{newline}"))
                    );
                }
                assert_reopens(&document, &saved);
                document.undo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), source);
                document.redo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), saved);
            }
        }
    }
    let source = include_str!("../../../performance/layout-fixtures/95-code-first-tree.md");
    let mut document = Document::from_markdown(source).unwrap();
    edit(
        &mut document,
        "let accepted = true;\nrecord(accepted);\n",
        "x",
    );
    let saved = document.snapshot().serialize().unwrap();
    assert_eq!(saved, source.replace("let accepted", "xlet accepted"));
    assert_reopens(&document, &saved);
}

#[test]
fn table_cell_edit_preserves_other_cells_and_source_delimiters() {
    check(
        "| Key | Value |\n| --- | --- |\n| One | a |\n| Two | Keep. |\n",
        "| Key | Value |\n| --- | --- |\n| One | xa |\n| Two | Keep. |\n",
    );
}

#[test]
fn table_source_spans_cover_unicode_crlf_empty_cells_and_inline_syntax() {
    for newline in ["\n", "\r\n"] {
        for source in [
            "Key|Value\n---|---\nÉtiquette|a\n別|  [keep][ref] &amp; **bold** `2.1`  \n\n[ref]: /local \"Exact title\"\n",
            "| Key | Value |\n| :--- | ---: |\n| a | |\n| Empty |   |\n",
            "> | Key | Value |\n> | --- | --- |\n> | One | a |\n> | Two | Keep. |\n",
            "| Key | Value |\n| --- | --- |\n| One | **a** |\n| Two | Keep: `a\\|b` &amp; [ref](/local). |\n",
        ] {
            let source = source.replace('\n', newline);
            let expected = if source.contains("**a**") {
                source.replace("**a**", "**xa**")
            } else if source.contains("Étiquette|a") {
                source.replace("Étiquette|a", "Étiquette|xa")
            } else {
                source.replacen("| a |", "| xa |", 1)
            };
            check(&source, &expected);
        }
    }
    let source = "| Key | Value |\n| --- | --- |\n| One | a |\n| Two | Keep. |\n";
    for inserted in [
        "x|", "**", "www.", "$", "&amp;", "東京 ", "<br>", "a@", "\\", "\n",
    ] {
        let mut document = Document::from_markdown(source).unwrap();
        edit(&mut document, "a", inserted);
        let saved = document.snapshot().serialize().unwrap();
        assert!(saved.contains("| Two | Keep. |"), "{saved}");
        assert_reopens(&document, &saved);
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }
    let source = "| Key | Value |\n| --- | --- |\n| Empty |   |\n| Other | Keep. |\n";
    let mut document = Document::from_markdown(source).unwrap();
    edit(&mut document, "", "x");
    let saved = document.snapshot().serialize().unwrap();
    assert!(saved.contains("| Other | Keep. |"), "{saved}");
    assert_reopens(&document, &saved);
    document.undo().unwrap();
    assert_eq!(document.snapshot().serialize().unwrap(), source);
}

#[test]
fn footnote_paragraph_edits_preserve_untouched_paragraphs_and_separators() {
    for newline in ["\n", "\r\n"] {
        let source = "Evidence[^note].\n\n[^note]: a\n\n    **Keep:**  [untouched][ref] &amp; `2.1`  \n    continued.\n\n    Last paragraph.\n\n[ref]: /local \"Title\"\n".replace('\n', newline);
        let expected = source.replacen("[^note]: a", "[^note]: xa", 1);
        check(&source, &expected);
    }
    for (source, expected) in [
        (
            "> Evidence[^n].\n>\n> [^n]: a\n>\n>     Keep: unchanged.\n",
            "> Evidence[^n].\n>\n> [^n]: xa\n>\n>     Keep: unchanged.\n",
        ),
        (
            "Evidence[^n].\n\n[^n]:\n    a\n\n    Keep: unchanged.\n",
            "Evidence[^n].\n\n[^n]:\n    xa\n\n    Keep: unchanged.\n",
        ),
        (
            "Evidence[^n].\n\n[^n]:\ta\n\n\tKeep: unchanged.\n",
            "Evidence[^n].\n\n[^n]:\txa\n\n\tKeep: unchanged.\n",
        ),
    ] {
        check(source, expected);
    }
}

#[test]
fn native_footnote_specimen_edit_preserves_every_other_byte() {
    let source = include_str!("../../../performance/layout-fixtures/66-footnotes.md");
    let mut document = Document::from_markdown(source).unwrap();
    edit(
        &mut document,
        "Field observation. Source labels can be descriptive and long. Display numbers stay compact and follow the order in which readers encounter the references.",
        "x",
    );
    let expected = source.replace(
        "**Field observation.** Source labels can be descriptive and long. Display numbers stay compact and follow the order in which readers encounter the references.",
        "**xField observation\\.** Source labels can be descriptive and long\\. Display numbers stay compact and follow the order in which readers encounter the references\\.",
    );
    let saved = document.snapshot().serialize().unwrap();
    assert_eq!(saved, expected);
    assert_reopens(&document, &saved);
    document.undo().unwrap();
    assert_eq!(document.snapshot().serialize().unwrap(), source);
}

#[test]
fn footnote_paragraph_continuations_reopen_without_rewriting_siblings() {
    for (source, original, inserted, expected) in [
        (
            "Evidence[^n].\n\n[^n]: a\n\n    Keep: unchanged.\n",
            "a",
            "x  \ny  \n",
            "Evidence[^n].\n\n[^n]: x  \n    y  \n    a\n\n    Keep: unchanged.\n",
        ),
        (
            "Evidence[^n].\n\n[^n]: Keep: unchanged.\n\n    a\n\n    Last: unchanged.\n",
            "a",
            "x  \ny  \n",
            "Evidence[^n].\n\n[^n]: Keep: unchanged.\n\n    x  \n    y  \n    a\n\n    Last: unchanged.\n",
        ),
        (
            "Evidence[^n].\n\n[^n]: a\n    wrapped  \n    line\n\n    Keep: unchanged.\n",
            "a wrapped  \nline",
            "x",
            "Evidence[^n].\n\n[^n]: xa wrapped  \n    line\n\n    Keep: unchanged.\n",
        ),
        (
            "> Evidence[^n].\n>\n> [^n]: a\n>\n>     Keep: unchanged.\n",
            "a",
            "x  \n",
            "> Evidence[^n].\n>\n> [^n]: x  \n>     a\n>\n>     Keep: unchanged.\n",
        ),
    ] {
        let mut document = Document::from_markdown(source).unwrap();
        edit(&mut document, original, inserted);
        let saved = document.snapshot().serialize().unwrap();
        assert_eq!(saved, expected);
        assert_reopens(&document, &saved);
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }
}

#[test]
fn editing_one_list_item_preserves_untouched_sibling_bytes() {
    let source = "- a\n- b:\n";
    let mut document = Document::from_markdown(source).unwrap();
    let snapshot = document.snapshot();
    let BlockNode::List(list) = snapshot.blocks().get(0).unwrap().as_ref() else {
        panic!("expected list");
    };
    let position = DocumentPosition::new(
        list.items[0].blocks.get(0).unwrap().id(),
        0,
        Affinity::Downstream,
    );
    document
        .apply(EditCommand::SetSelection(Selection::Text(TextSelection {
            anchor: position,
            head: position,
        })))
        .unwrap();
    document
        .apply(EditCommand::ReplaceSelection {
            text: "x".into(),
            typing: false,
        })
        .unwrap();
    assert_eq!(document.snapshot().serialize().unwrap(), "- xa\n- b:\n");
}

#[test]
fn item_reuse_keeps_authored_dialect_spacing_links_and_line_endings() {
    for (source, expected) in [
        (
            "+ a\n+ **Label:**  [untouched][ref] &amp; `2.1`  \n  continued\n\n[ref]: /local \"Title\"\n",
            "+ xa\n+ **Label:**  [untouched][ref] &amp; `2.1`  \n  continued\n\n[ref]: /local \"Title\"\n",
        ),
        ("07) a\n20) untouched:\n", "07) xa\n20) untouched:\n"),
        (
            "*  a\r\n\r\n*  **Keep:** untouched.\r\n",
            "*  xa\r\n\r\n*  **Keep:** untouched.\r\n",
        ),
        (
            "- parent:\n  + a\n  + b:\n- outside:\n",
            "- parent:\n  + xa\n  + b:\n- outside:\n",
        ),
        ("> - a\n> - b:\n", "> - xa\n> - b:\n"),
        ("> [!NOTE]\n> - a\n> - b:\n", "> [!NOTE]\n> - xa\n> - b:\n"),
        ("Term\n: - a\n  - b:\n", "Term\n: - xa\n  - b:\n"),
        ("* [X] a\n* [ ] b:\n", "* [X] xa\n* [ ] b:\n"),
        ("- [x] done:\n- a\n", "- [x] done:\n- xa\n"),
        ("- a\n- b:", "- xa\n- b:"),
        ("-\ta\n-\tb:\n", "-\txa\n-\tb:\n"),
        ("1. [ ] a\n2. [x] b:\n", "1. [ ] xa\n2. [x] b:\n"),
    ] {
        check(source, expected);
    }
}

#[test]
fn multiple_edits_reuse_only_unchanged_items_from_the_original_snapshot() {
    let source = "- a\n- b:\n- c:\n";
    let mut document = Document::from_markdown(source).unwrap();
    edit(&mut document, "a", "x");
    edit(&mut document, "b:", "y");
    assert_eq!(
        document.snapshot().serialize().unwrap(),
        "- xa\n- yb\\:\n- c:\n"
    );
    document.undo().unwrap();
    assert_eq!(
        document.snapshot().serialize().unwrap(),
        "- xa\n- b:\n- c:\n"
    );
    document.undo().unwrap();
    assert_eq!(document.snapshot().serialize().unwrap(), source);
}

#[test]
fn toggling_a_task_preserves_its_untouched_body_spelling() {
    fn task(block: &BlockNode) -> Option<(NodeId, bool)> {
        match block {
            BlockNode::List(list) => list.items.iter().find_map(|item| {
                item.checked
                    .map(|checked| (item.id, checked))
                    .or_else(|| item.blocks.iter().find_map(|block| task(block)))
            }),
            BlockNode::BlockQuote { blocks, .. } => blocks.iter().find_map(|block| task(block)),
            _ => None,
        }
    }
    for source in [
        "- [x] task.\n",
        "* [X] **Keep:** done. &amp; _emphasis_\r\n* [ ] sibling:\r\n",
        "3) [ ] [Link](https://example.org) and `code()`!\n4) [x] sibling\n",
        "- parent\n  + [x] nested.\n    continuation: stays!\n",
        "> - [x] quoted.\n>\n>   A second paragraph.\n>\n>   - Nested child.\n",
        "-\t[ ]\ttask.  \r\n\tcontinued.\r\n",
        include_str!("../../../performance/layout-fixtures/53-document-grammar.md"),
    ] {
        let mut document = Document::from_markdown(source).unwrap();
        let before = document.snapshot();
        let (item_id, checked) = before
            .blocks()
            .iter()
            .find_map(|block| task(block))
            .unwrap();
        let marker_start = source
            .find(if checked {
                if source.contains("[X]") { "[X]" } else { "[x]" }
            } else {
                "[ ]"
            })
            .unwrap();
        let mut expected = source.to_owned();
        expected.replace_range(
            marker_start + 1..marker_start + 2,
            if checked { " " } else { "x" },
        );
        document.apply(EditCommand::ToggleTask { item_id }).unwrap();
        let after = document.snapshot();
        assert_eq!(
            after
                .blocks()
                .iter()
                .map(|block| block.plain_text())
                .collect::<Vec<_>>(),
            before
                .blocks()
                .iter()
                .map(|block| block.plain_text())
                .collect::<Vec<_>>()
        );
        let saved = after.serialize().unwrap();
        assert_eq!(saved, expected);
        let reopened = Document::from_markdown(saved.as_str()).unwrap();
        assert_eq!(
            reopened
                .snapshot()
                .blocks()
                .iter()
                .find_map(|block| task(block))
                .unwrap()
                .1,
            !checked
        );
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }
}

#[test]
fn toggling_a_task_changes_its_state_without_rewriting_siblings() {
    let source = "* [ ] first\n* [X] **Keep:** done.\n";
    let mut document = Document::from_markdown(source).unwrap();
    let snapshot = document.snapshot();
    let BlockNode::List(list) = snapshot.blocks().get(0).unwrap().as_ref() else {
        panic!()
    };
    document
        .apply(EditCommand::ToggleTask {
            item_id: list.items[0].id,
        })
        .unwrap();
    let saved = document.snapshot().serialize().unwrap();
    assert_eq!(saved, "* [x] first\n* [X] **Keep:** done.\n");
    let reopened = Document::from_markdown(saved.as_str()).unwrap();
    let after = reopened.snapshot();
    let BlockNode::List(list) = after.blocks().get(0).unwrap().as_ref() else {
        panic!()
    };
    assert!(list.items.iter().all(|item| item.checked == Some(true)));
    document.undo().unwrap();
    assert_eq!(document.snapshot().serialize().unwrap(), source);
}

#[test]
fn multiline_edits_keep_list_and_quote_continuation_context() {
    for source in [
        "- a\n- b:\n",
        "> - a\n> - b:\n",
        "- parent:\n  + a\n  + b:\n",
        "- a\r\n- b:\r\n",
    ] {
        let mut document = Document::from_markdown(source).unwrap();
        edit(&mut document, "a", "first  \nsecond  \n");
        let saved = document.snapshot().serialize().unwrap();
        assert_reopens(&document, &saved);
        let ending = if source.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        assert!(saved.ends_with(&format!("{}{ending}", source.lines().last().unwrap())));
        if ending == "\r\n" {
            assert!(!saved.replace("\r\n", "").contains('\n'));
        }
    }
}

#[test]
fn structural_edits_are_not_hidden_by_original_item_spans() {
    for operation in [0, 1, 2] {
        let source = "- a\n- **Keep:** b.\n";
        let mut document = Document::from_markdown(source).unwrap();
        let snapshot = document.snapshot();
        let BlockNode::List(list) = snapshot.blocks().get(0).unwrap().as_ref() else {
            panic!()
        };
        let id = list.items[0].blocks.get(0).unwrap().id();
        match operation {
            0 => {
                document
                    .apply(EditCommand::SetBlockStyle {
                        node_id: id,
                        style: document_core::BlockStyle::Heading(3),
                    })
                    .unwrap();
            }
            1 => {
                document
                    .apply(EditCommand::ReplaceText {
                        node_id: id,
                        range: 0..1,
                        text: "new".into(),
                        selection_after: None,
                        typing: false,
                    })
                    .unwrap();
            }
            _ => {
                document
                    .apply(EditCommand::IndentListItem {
                        item_id: list.items[1].id,
                    })
                    .unwrap();
            }
        }
        let saved = document.snapshot().serialize().unwrap();
        assert_ne!(saved, source);
        assert_reopens(&document, &saved);
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }
}

#[test]
fn converted_html_keeps_preceding_source_records_at_the_replacement_boundary() {
    let source = concat!(
        "Title\n=====\n\n",
        "[ref]: /exact-destination \"Exact title\"\n\n",
        "<div><p>convert <em>me</em></p><p>second</p></div>\n\n",
        "~~~rust\ncode\n~~~\n"
    );
    let mut document = Document::from_markdown(source).expect("document");
    let html_id = document
        .snapshot()
        .blocks()
        .iter()
        .find(|block| matches!(block.as_ref(), BlockNode::PreservedSource { .. }))
        .expect("preserved HTML")
        .id();

    document
        .apply(EditCommand::ConvertHtmlToMarkdown { node_id: html_id })
        .expect("convert HTML");
    let saved = document.snapshot().serialize().expect("serialize");

    let definition = saved.find("[ref]:").expect("reference definition");
    let converted = saved.find("convert *me*").expect("converted paragraph");
    assert!(
        definition < converted,
        "source record moved across its replacement: {saved:?}"
    );
    assert!(
        saved.starts_with("Title\n=====\n"),
        "setext changed: {saved:?}"
    );
    assert!(
        saved.ends_with("~~~rust\ncode\n~~~\n"),
        "fence changed: {saved:?}"
    );
    assert_reopens(&document, &saved);

    let converted_saved = saved;
    document.undo().expect("undo conversion");
    assert_eq!(document.snapshot().serialize().unwrap(), source);
    document.redo().expect("redo conversion");
    assert_eq!(document.snapshot().serialize().unwrap(), converted_saved);
}

#[test]
fn moving_the_first_block_regenerates_only_changed_boundaries() {
    let source = concat!(
        "Title\n=====\n\n",
        "Body with **meaning**.\n\n",
        "~~~rust\nlet value = 1;\n~~~\n"
    );
    let mut document = Document::from_markdown(source).expect("document");
    let selection = document.snapshot().selection().clone();

    document
        .apply(EditCommand::MoveBlock { from: 0, to: 2 })
        .expect("move heading");
    let saved = document.snapshot().serialize().expect("serialize");

    assert!(saved.contains("Title\n====="), "setext changed: {saved:?}");
    assert!(
        saved.contains("~~~rust\nlet value = 1;\n~~~"),
        "fence changed: {saved:?}"
    );
    assert_reopens(&document, &saved);

    let moved = saved;
    document.undo().expect("undo move");
    assert_eq!(document.snapshot().serialize().unwrap(), source);
    assert_eq!(document.snapshot().selection(), &selection);
    document.redo().expect("redo move");
    assert_eq!(document.snapshot().serialize().unwrap(), moved);
}

#[test]
fn joining_blocks_preserves_mixed_ending_source_records_and_inline_meaning() {
    let source = concat!(
        "---\r\nname: exact\r\n---\r\n\r\n",
        "Title\n=====\n\n",
        "Before **bold**.\r\n\r\n",
        "Join *start*.\n\n",
        "[ref]: /exact-destination \"Exact title\"\r\n\r\n",
        "<!-- exact comment -->\n\n",
        "~~~rust\r\nlet value = 1;\r\n~~~\r\n"
    );
    let mut document = Document::from_markdown(source).expect("document");
    let before = document.snapshot();
    let first = before
        .blocks()
        .iter()
        .find(|block| block.plain_text() == "Before bold.")
        .expect("first paragraph")
        .id();
    let second = before
        .blocks()
        .iter()
        .find(|block| block.plain_text() == "Join start.")
        .expect("second paragraph")
        .id();
    let selection = TextSelection {
        anchor: DocumentPosition::new(first, "Before ".len(), Affinity::Downstream),
        head: DocumentPosition::new(second, "Join ".len(), Affinity::Upstream),
    };
    document
        .apply(EditCommand::SetSelection(Selection::Text(
            selection.clone(),
        )))
        .expect("selection");
    document
        .apply(EditCommand::ReplaceSelection {
            text: "connected ".into(),
            typing: false,
        })
        .expect("join paragraphs");

    let saved = document.snapshot().serialize().expect("serialize");
    for exact in [
        "---\r\nname: exact\r\n---",
        "Title\n=====",
        "[ref]: /exact-destination \"Exact title\"\r\n",
        "<!-- exact comment -->\n",
        "~~~rust\r\nlet value = 1;\r\n~~~\r\n",
    ] {
        assert!(
            saved.contains(exact),
            "lost exact source {exact:?}: {saved:?}"
        );
    }
    assert_reopens(&document, &saved);

    let joined = saved;
    document.undo().expect("undo join");
    assert_eq!(document.snapshot().serialize().unwrap(), source);
    assert_eq!(document.snapshot().selection(), &Selection::Text(selection));
    document.redo().expect("redo join");
    assert_eq!(document.snapshot().serialize().unwrap(), joined);
}

#[test]
fn inserted_nonempty_block_preserves_surrounding_source_units() {
    let source = concat!(
        "Title\n=====\n\n",
        "Body [reference][ref].\n\n",
        "[ref]: /exact \"Title\"\n\n",
        "<!-- keep exact -->\n\n",
        "````rust\ncode\n````\n"
    );
    let mut document = Document::from_markdown(source).expect("document");
    let heading = document.snapshot().blocks().get(0).expect("heading").id();
    let selection = Selection::Text(TextSelection::caret(DocumentPosition::new(
        heading,
        "Title".len(),
        Affinity::Downstream,
    )));
    document
        .apply(EditCommand::SetSelection(selection.clone()))
        .expect("selection");
    document
        .apply(EditCommand::InsertBlockAfterSelection {
            kind: document_core::InsertBlockKind::Paragraph,
        })
        .expect("insert paragraph");
    document
        .apply(EditCommand::ReplaceSelection {
            text: "Inserted **literally**".into(),
            typing: false,
        })
        .expect("fill paragraph");

    let saved = document.snapshot().serialize().expect("serialize");
    assert!(
        saved.starts_with("Title\n=====\n"),
        "setext changed: {saved:?}"
    );
    assert!(saved.contains("[ref]: /exact \"Title\"\n"));
    assert!(saved.contains("<!-- keep exact -->\n"));
    assert!(saved.ends_with("````rust\ncode\n````\n"));
    assert_reopens(&document, &saved);

    document.undo().expect("undo inserted text");
    document.undo().expect("undo inserted block");
    assert_eq!(document.snapshot().serialize().unwrap(), source);
    assert_eq!(document.snapshot().selection(), &selection);
    document.redo().expect("redo inserted block");
    document.redo().expect("redo inserted text");
    assert_eq!(document.snapshot().serialize().unwrap(), saved);
}

#[test]
fn html_table_header_counts_and_nested_list_content_survive_presentation_edits() {
    for headers in [0, 1, 2] {
        for newline in ["\n", "\r\n"] {
            let mut source = String::from("<table>\n");
            for index in 0..headers {
                source.push_str(&format!("<tr><th>Header {index}</th></tr>\n"));
            }
            source.push_str("<tr><td>Value</td></tr>\n</table>\n");
            let source = source.replace('\n', newline);
            let mut document = Document::from_markdown(source.as_str()).unwrap();
            let id = document.snapshot().blocks().get(0).unwrap().id();
            document
                .apply(EditCommand::SetTableBorder {
                    table_id: id,
                    border: document_core::TableBorder::Dotted,
                })
                .unwrap();
            let saved = document.snapshot().serialize().unwrap();
            assert_reopens(&document, &saved);
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), source);
            document.redo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), saved);
        }
    }

    let source = concat!(
        "<table><tr><th>List</th></tr><tr><td>",
        "<ol start=\"12\"><li><p><strong>first</strong> <em>item</em></p>",
        "<p>second paragraph</p><ul><li>nested one</li><li>nested two</li></ul>",
        "<pre><code class=\"language-rust\">let x = 1;\n</code></pre></li>",
        "<li>last item</li></ol></td></tr></table>\n"
    );
    let mut document = Document::from_markdown(source).unwrap();
    let id = document.snapshot().blocks().get(0).unwrap().id();
    document
        .apply(EditCommand::SetTableBorder {
            table_id: id,
            border: document_core::TableBorder::Dotted,
        })
        .unwrap();
    let saved = document.snapshot().serialize().unwrap();
    assert_reopens(&document, &saved);
    let snapshot = document.snapshot();
    let BlockNode::Table(table) = snapshot.node(id).unwrap() else {
        panic!("table")
    };
    let BlockNode::List(list) = table.rows[1].cells[0].blocks.get(0).unwrap().as_ref() else {
        panic!("list")
    };
    assert_eq!(list.items.len(), 2);
    assert_eq!(list.items[0].blocks.len(), 4);
    assert_eq!(
        list.items[0].blocks.get(0).unwrap().plain_text(),
        "first item"
    );
    document.undo().unwrap();
    assert_eq!(document.snapshot().serialize().unwrap(), source);
}

#[test]
fn html_table_linked_figures_keep_destinations_and_titles() {
    let source = concat!(
        "<table><tr><th>Figure</th></tr><tr><td>",
        "<p><a href=\"full.png?a=1&amp;b=2\" title=\"Full &quot;size&quot;\">",
        "<img src=\"thumb.png?a=1&amp;b=2\" alt=\"A &amp; B\" title=\"Preview\">",
        "</a></p><p>Caption</p></td></tr></table>\n"
    );
    let check_figure = |document: &Document| {
        let snapshot = document.snapshot();
        let BlockNode::Table(table) = snapshot.blocks().get(0).unwrap().as_ref() else {
            panic!("table")
        };
        let BlockNode::Image(image) = table.rows[1].cells[0].blocks.get(0).unwrap().as_ref() else {
            panic!("figure")
        };
        assert_eq!(image.source, "thumb.png?a=1&b=2");
        assert_eq!(image.alt.as_string(), "A & B");
        assert_eq!(image.title.as_deref(), Some("Preview"));
        let link = image.link.as_ref().expect("figure link");
        assert_eq!(link.target.0, "full.png?a=1&b=2");
        assert_eq!(link.title.as_deref(), Some("Full \"size\""));
    };
    for newline in ["\n", "\r\n"] {
        let source = source.replace('\n', newline);
        let mut document = Document::from_markdown(source.as_str()).unwrap();
        check_figure(&document);
        let id = document.snapshot().blocks().get(0).unwrap().id();
        document
            .apply(EditCommand::SetTableBorder {
                table_id: id,
                border: document_core::TableBorder::Dotted,
            })
            .unwrap();
        check_figure(&document);
        let saved = document.snapshot().serialize().unwrap();
        assert_reopens(&document, &saved);
        check_figure(&Document::from_markdown(saved.as_str()).unwrap());
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        document.redo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), saved);
    }
}

#[test]
fn successful_save_rebases_source_without_changing_content_or_history() {
    let original = "Title\r\n=====\r\n\r\n- Alpha\r\n- **Beta**\r\n\r\n<!-- keep -->\n";
    let mut document = Document::from_markdown(original).unwrap();
    let initial_selection = document.snapshot().selection().clone();
    edit(&mut document, "Alpha", "New ");
    let before = document.snapshot();
    let prepared = before.prepare_source_rebase().unwrap();
    let saved = prepared.bytes().to_vec();
    assert!(document.rebase_source(prepared));
    let after = document.snapshot();
    assert_eq!(after.revision(), before.revision());
    assert_eq!(after.selection(), before.selection());
    for (old, new) in before.blocks().iter().zip(after.blocks()) {
        assert!(std::sync::Arc::ptr_eq(old, new));
    }
    assert!(after.dirty_node_ids().is_empty());
    assert_eq!(after.source_spine().original().as_bytes(), saved);
    assert_eq!(after.serialize().unwrap().as_bytes(), saved);
    edit(&mut document, "Beta", "Next ");
    let twice = document.snapshot().serialize().unwrap();
    assert!(twice.contains("- New Alpha\r\n"));
    assert!(twice.contains("<!-- keep -->\n"));
    document.undo().unwrap();
    assert_eq!(document.snapshot().serialize().unwrap().as_bytes(), saved);
    document.undo().unwrap();
    assert_eq!(document.snapshot().serialize().unwrap(), original);
    assert_eq!(document.snapshot().selection(), &initial_selection);
    document.redo().unwrap();
    assert_eq!(document.snapshot().serialize().unwrap().as_bytes(), saved);
}

#[test]
fn save_rebase_rejects_newer_edits_and_unrelated_documents() {
    let mut document = Document::from_markdown("Alpha\n").unwrap();
    edit(&mut document, "Alpha", "New ");
    let prepared = document.snapshot().prepare_source_rebase().unwrap();
    edit(&mut document, "New Alpha", "More ");
    assert!(!document.rebase_source(prepared));
    assert!(!document.snapshot().dirty_node_ids().is_empty());
    assert_eq!(document.snapshot().source_spine().original(), "Alpha\n");
    let prepared = document.snapshot().prepare_source_rebase().unwrap();
    let mut other = Document::from_markdown("Alpha\n").unwrap();
    edit(&mut other, "Alpha", "New ");
    edit(&mut other, "New Alpha", "More ");
    assert_eq!(other.snapshot().revision(), document.snapshot().revision());
    assert!(!other.rebase_source(prepared));
}

#[test]
fn rebased_nested_ranges_preserve_second_edit_and_structural_history() {
    for original in [
        "| A | B |\n| --- | --- |\n| Alpha | **Beta** |\n",
        "<table class='keep'><tbody><tr><td><p>Alpha</p></td><td><p><b>Beta</b></p></td></tr></tbody></table>\n",
        "> Alpha\n>\n> **Beta**\n",
    ] {
        let mut document = Document::from_markdown(original).unwrap();
        edit(&mut document, "Alpha", "New ");
        let first = document.snapshot().serialize().unwrap();
        assert!(
            document.rebase_source(document.snapshot().prepare_source_rebase().unwrap()),
            "{first}"
        );
        edit(&mut document, "Beta", "Next ");
        let saved = document.snapshot().serialize().unwrap();
        assert!(saved.contains("New Alpha"));
        if original.starts_with("<table") {
            assert_eq!(
                saved,
                original
                    .replace("Alpha", "New Alpha")
                    .replace("<b>Beta</b>", "<b>Next Beta</b>")
            );
        }
        assert!(document.rebase_source(document.snapshot().prepare_source_rebase().unwrap()));
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), first);
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), original);
        document.redo().unwrap();
        document.redo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), saved);
    }
    let mut document = Document::from_markdown("Alpha\n\nTitle\n=====\n").unwrap();
    let id = document.snapshot().blocks().get(0).unwrap().id();
    let selection = Selection::Text(TextSelection::caret(DocumentPosition::new(
        id,
        2,
        Affinity::Downstream,
    )));
    document
        .apply(EditCommand::SetSelection(selection.clone()))
        .unwrap();
    document.apply(EditCommand::SplitSelection).unwrap();
    let saved = document.snapshot().serialize().unwrap();
    assert!(document.rebase_source(document.snapshot().prepare_source_rebase().unwrap()));
    assert_eq!(document.snapshot().source_spine().original(), saved);
    assert_eq!(
        Document::from_markdown(saved.clone())
            .unwrap()
            .snapshot()
            .blocks()
            .len(),
        3
    );
    document.undo().unwrap();
    assert_eq!(document.snapshot().selection(), &selection);
    assert_eq!(
        document.snapshot().serialize().unwrap(),
        "Alpha\n\nTitle\n=====\n"
    );
    document.redo().unwrap();
    assert_eq!(document.snapshot().serialize().unwrap(), saved);
}

#[test]
fn save_rebase_preserves_selection_and_defers_unrepresentable_or_composing_state() {
    let mut document = Document::from_markdown("Alpha\n").unwrap();
    edit(&mut document, "Alpha", "New ");
    let prepared = document.snapshot().prepare_source_rebase().unwrap();
    let id = document.snapshot().blocks().get(0).unwrap().id();
    let caret = TextSelection::caret(DocumentPosition::new(id, 2, Affinity::Downstream));
    document
        .apply(EditCommand::SetSelection(Selection::Text(caret.clone())))
        .unwrap();
    assert!(document.rebase_source(prepared));
    assert_eq!(
        document.snapshot().selection(),
        &Selection::Text(caret.clone())
    );
    let prepared = document.snapshot().prepare_source_rebase().unwrap();
    document.begin_composition(caret).unwrap();
    assert!(!document.rebase_source(prepared));
    document.cancel_composition().unwrap();
    let end = TextSelection::caret(DocumentPosition::new(id, 9, Affinity::Downstream));
    document
        .apply(EditCommand::SetSelection(Selection::Text(end)))
        .unwrap();
    document.apply(EditCommand::SplitSelection).unwrap();
    let before = document.snapshot();
    assert_eq!(before.blocks().len(), 2);
    let prepared = before.prepare_source_rebase().unwrap();
    assert!(!document.rebase_source(prepared));
    assert_eq!(
        document.snapshot().dirty_node_ids(),
        before.dirty_node_ids()
    );
    assert_eq!(
        document.snapshot().serialize().unwrap(),
        before.serialize().unwrap()
    );
}

#[test]
fn structural_split_does_not_duplicate_nested_footnote_source() {
    for source in [
        "> Evidence[^n].\n>\n> [^n]: Keep **note**.\n\nAfter\n",
        "- Evidence[^n].\n\n  [^n]: Keep **note**.\n\nAfter\n",
        "> Evidence[^n] and more[^m].\n>\n> [^n]: Keep **note**.\n>\n> [^m]: Second note.\n\nAfter\n",
    ] {
        let mut document = Document::from_markdown(source).unwrap();
        let id = document
            .snapshot()
            .blocks()
            .iter()
            .find_map(|b| leaf(b, "After"))
            .unwrap();
        let selection = Selection::Text(TextSelection::caret(DocumentPosition::new(
            id,
            2,
            Affinity::Downstream,
        )));
        document
            .apply(EditCommand::SetSelection(selection.clone()))
            .unwrap();
        document.apply(EditCommand::SplitSelection).unwrap();
        let saved = document.snapshot().serialize().unwrap();
        assert_eq!(saved.matches("[^n]:").count(), 1, "{saved:?}");
        assert_eq!(
            saved.matches("[^m]:").count(),
            source.matches("[^m]:").count()
        );
        assert!(
            saved.starts_with(source.split("After").next().unwrap()),
            "{saved:?}"
        );
        assert_reopens(&document, &saved);
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        assert_eq!(document.snapshot().selection(), &selection);
        document.redo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), saved);
    }
}

#[test]
fn lifted_note_source_ownership_survives_moves_edits_and_deletions() {
    for newline in ["\n", "\r\n"] {
        for prefix in [
            "> Evidence[^n].\n>\n> [^n]: Keep **note**.\n",
            "- Evidence[^n].\n\n  [^n]: Keep **note**.\n",
        ] {
            let source = format!("{prefix}\nAfter\n\nTitle\n=====\n").replace('\n', newline);
            for operation in 0..4 {
                let mut document = Document::from_markdown(source.clone()).unwrap();
                let before = document.snapshot();
                let owner = before.blocks().get(0).unwrap().id();
                let note = before.blocks().get(1).unwrap().id();
                assert!(matches!(
                    before.node(note),
                    Some(BlockNode::FootnoteDefinition { .. })
                ));
                match operation {
                    0 => {
                        document
                            .apply(EditCommand::MoveBlock { from: 0, to: 3 })
                            .unwrap();
                    }
                    1 => {
                        document
                            .apply(EditCommand::MoveBlock { from: 1, to: 3 })
                            .unwrap();
                    }
                    2 => {
                        document
                            .apply(EditCommand::DeleteBlock { node_id: owner })
                            .unwrap();
                    }
                    _ => {
                        document
                            .apply(EditCommand::DeleteBlock { node_id: note })
                            .unwrap();
                    }
                }
                let saved = document.snapshot().serialize().unwrap();
                assert_eq!(
                    saved.matches("[^n]:").count(),
                    usize::from(operation != 3),
                    "{operation}: {saved:?}"
                );
                assert!(
                    saved.contains(&format!("Title{newline}====={newline}")),
                    "{saved:?}"
                );
                if operation < 2 {
                    assert_reopens(&document, &saved);
                    assert!(
                        document
                            .rebase_source(document.snapshot().prepare_source_rebase().unwrap())
                    );
                }
                document.undo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), source);
                document.redo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), saved);
            }
            let mut document = Document::from_markdown(source.clone()).unwrap();
            edit(&mut document, "Keep note.", "New ");
            let after = document
                .snapshot()
                .blocks()
                .iter()
                .find_map(|b| leaf(b, "After"))
                .unwrap();
            document
                .apply(EditCommand::SetSelection(Selection::Text(
                    TextSelection::caret(DocumentPosition::new(after, 2, Affinity::Downstream)),
                )))
                .unwrap();
            document.apply(EditCommand::SplitSelection).unwrap();
            let saved = document.snapshot().serialize().unwrap();
            assert_eq!(saved.matches("[^n]:").count(), 1, "{saved:?}");
            assert!(saved.contains("New Keep"));
            assert_reopens(&document, &saved);
            assert!(document.rebase_source(document.snapshot().prepare_source_rebase().unwrap()));
            edit(&mut document, "New Keep note.", "Next ");
            let saved = document.snapshot().serialize().unwrap();
            assert_eq!(saved.matches("[^n]:").count(), 1, "{saved:?}");
            assert_reopens(&document, &saved);
        }
    }
}

#[test]
fn deleting_container_preserves_reference_definitions_used_outside_it() {
    for source in [
        "- Before\n\n  [ref]: /local \"Title\"\n\nOutside [link][ref].\n",
        "> Before\n>\n> [ref]: /local \"Title\"\n\nOutside [link][ref].\n",
        "> [ref]: /local \"Title\"\n> Before\n\nOutside [link][ref].\n",
        "- Before\r\n\r\n  [ref]: /local \"Title\"\r\n\r\nOutside [link][ref].\r\n",
    ] {
        let mut document = Document::from_markdown(source).unwrap();
        let before = document.snapshot();
        let outside = before
            .blocks()
            .iter()
            .find_map(|b| leaf(b, "Outside link."))
            .unwrap();
        let runs = before
            .node(outside)
            .unwrap()
            .text()
            .unwrap()
            .runs()
            .to_vec();
        document
            .apply(EditCommand::DeleteBlock {
                node_id: before.blocks().get(0).unwrap().id(),
            })
            .unwrap();
        let saved = document.snapshot().serialize().unwrap();
        assert!(saved.contains("[ref]: /local \"Title\""), "{saved:?}");
        let reopened = Document::from_markdown(saved.clone()).unwrap();
        let snapshot = reopened.snapshot();
        let id = snapshot
            .blocks()
            .iter()
            .find_map(|b| leaf(b, "Outside link."))
            .expect("reference must still resolve");
        assert_eq!(snapshot.node(id).unwrap().text().unwrap().runs(), runs);
        assert_eq!(
            snapshot.blocks().len(),
            1,
            "no deleted container resurrected"
        );
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        document.redo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), saved);
    }
}

#[test]
fn removed_reference_records_keep_multiline_titles_and_ignore_code_examples() {
    for source in [
        "> Before\n>\n> [ref]:\n>   /local\n>   \"Long\n>     title\"\n\nOutside [link][ref].\n",
        "- Before\n\n  [ref]: /local\n    'Long title'\n\nOutside [link][ref].\n",
    ] {
        let mut document = Document::from_markdown(source).unwrap();
        let before = document.snapshot();
        let outside = before
            .blocks()
            .iter()
            .find_map(|b| leaf(b, "Outside link."))
            .unwrap();
        let runs = before
            .node(outside)
            .unwrap()
            .text()
            .unwrap()
            .runs()
            .to_vec();
        document
            .apply(EditCommand::DeleteBlock {
                node_id: before.blocks().get(0).unwrap().id(),
            })
            .unwrap();
        let saved = document.snapshot().serialize().unwrap();
        let reopened = Document::from_markdown(saved.clone()).unwrap().snapshot();
        let outside = reopened
            .blocks()
            .iter()
            .find_map(|b| leaf(b, "Outside link."))
            .expect("multiline definition retained");
        assert_eq!(
            reopened.node(outside).unwrap().text().unwrap().runs(),
            runs,
            "{saved}"
        );
        assert_eq!(reopened.blocks().len(), 1);
        assert!(document.rebase_source(document.snapshot().prepare_source_rebase().unwrap()));
        edit(&mut document, "Outside link.", "New ");
        let saved = document.snapshot().serialize().unwrap();
        assert!(saved.contains("[ref]:"));
        document.undo().unwrap();
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }
    for source in [
        "> ```md\n> [ref]: /wrong\n> ```\n\nOutside [link][ref].\n\n[ref]: /right\n",
        "> <pre>\n> [ref]: /wrong\n> </pre>\n\nOutside [link][ref].\n\n[ref]: /right\n",
        "> Before\n> [ref]: /wrong\n\nOutside [link][ref].\n\n[ref]: /right\n",
        "> Before\n> [ref]: /wrong\\path\n\nOutside [link][ref].\n\n[ref]: /right\n",
        "---\n[ref]: /wrong\n---\n\nOutside [link][ref].\n\n[ref]: /right\n",
    ] {
        let mut document = Document::from_markdown(source).unwrap();
        document
            .apply(EditCommand::DeleteBlock {
                node_id: document.snapshot().blocks().get(0).unwrap().id(),
            })
            .unwrap();
        let saved = document.snapshot().serialize().unwrap();
        assert!(
            !saved.contains("/wrong"),
            "code/HTML example was activated: {saved}"
        );
        assert!(saved.contains("[ref]: /right"));
    }
}

#[test]
fn splitting_inside_container_retains_reference_records() {
    for source in [
        "> Before\n>\n> [ref]: /local \"Title\"\n\nOutside [link][ref].\n",
        "- Before\n\n  [ref]: /local \"Title\"\n  More\n\nOutside [link][ref].\n",
    ] {
        let mut document = Document::from_markdown(source).unwrap();
        let original = document.snapshot();
        let id = original
            .blocks()
            .iter()
            .find_map(|b| leaf(b, "Before"))
            .unwrap();
        let outside = original
            .blocks()
            .iter()
            .find_map(|b| leaf(b, "Outside link."))
            .unwrap();
        let runs = original
            .node(outside)
            .unwrap()
            .text()
            .unwrap()
            .runs()
            .to_vec();
        let selection = Selection::Text(TextSelection::caret(DocumentPosition::new(
            id,
            2,
            Affinity::Downstream,
        )));
        document
            .apply(EditCommand::SetSelection(selection.clone()))
            .unwrap();
        document.apply(EditCommand::SplitSelection).unwrap();
        let saved = document.snapshot().serialize().unwrap();
        assert_eq!(saved.matches("[ref]:").count(), 1, "{saved}");
        let reopened = Document::from_markdown(saved.clone()).unwrap().snapshot();
        let outside = reopened
            .blocks()
            .iter()
            .find_map(|b| leaf(b, "Outside link."))
            .expect("reference still resolves");
        assert_eq!(reopened.node(outside).unwrap().text().unwrap().runs(), runs);
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        assert_eq!(document.snapshot().selection(), &selection);
        document.redo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), saved);
    }
}

#[test]
fn regenerated_reference_records_preserve_precedence_and_lifted_notes() {
    for newline in ["\n", "\r\n"] {
        for body in [
            "> [ref]: /local \"Title\"\n> Before\n",
            "> Before\n>\n> [ref]: /local \"Title\"\n> [REF]: /ignored \"Second\"\n",
            "> Before[^n]\n>\n> [ref]: /local \"Title\"\n>\n> [^n]: Keep **note**.\n",
        ] {
            let source =
                format!("Title\n=====\n\n{body}\nOutside [link][ref].\n\n~~~rust\ncode\n~~~\n")
                    .replace('\n', newline);
            for operation in 0..3 {
                let mut document = Document::from_markdown(source.clone()).unwrap();
                let original = document.snapshot();
                let text = if body.contains("[^n]") {
                    "Before[^n]"
                } else {
                    "Before"
                };
                let id = original
                    .blocks()
                    .iter()
                    .find_map(|b| leaf(b, text))
                    .unwrap();
                let outside = original
                    .blocks()
                    .iter()
                    .find_map(|b| leaf(b, "Outside link."))
                    .unwrap();
                let runs = original
                    .node(outside)
                    .unwrap()
                    .text()
                    .unwrap()
                    .runs()
                    .to_vec();
                match operation {
                    0 => {
                        document
                            .apply(EditCommand::SetBlockStyle {
                                node_id: id,
                                style: document_core::BlockStyle::Heading(3),
                            })
                            .unwrap();
                    }
                    1 => {
                        document
                            .apply(EditCommand::SetSelection(Selection::Text(
                                TextSelection::caret(DocumentPosition::new(
                                    id,
                                    2,
                                    Affinity::Downstream,
                                )),
                            )))
                            .unwrap();
                        document.apply(EditCommand::SplitSelection).unwrap();
                    }
                    _ => {
                        edit(&mut document, text, "New ");
                    }
                }
                let saved = document.snapshot().serialize().unwrap();
                assert_eq!(saved.matches("[ref]:").count(), 1, "{saved}");
                assert_eq!(
                    saved.matches("[REF]:").count(),
                    usize::from(body.contains("[REF]")),
                    "{saved}"
                );
                assert_eq!(
                    saved.matches("[^n]:").count(),
                    usize::from(body.contains("[^n]")),
                    "{saved}"
                );
                assert!(saved.starts_with(&format!("Title{newline}====={newline}")));
                assert!(saved.ends_with(&format!("~~~rust{newline}code{newline}~~~{newline}")));
                let reopened = Document::from_markdown(saved.clone()).unwrap().snapshot();
                let outside = reopened
                    .blocks()
                    .iter()
                    .find_map(|b| leaf(b, "Outside link."))
                    .expect("reference retained");
                assert_eq!(
                    reopened.node(outside).unwrap().text().unwrap().runs(),
                    runs,
                    "{saved}"
                );
                assert_reopens(&document, &saved);
                assert!(
                    document.rebase_source(document.snapshot().prepare_source_rebase().unwrap()),
                    "{saved}"
                );
                edit(&mut document, "Outside link.", "Next ");
                let next = document.snapshot().serialize().unwrap();
                assert_eq!(next.matches("[ref]:").count(), 1);
                assert_reopens(&document, &next);
                document.undo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), saved);
                document.undo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), source);
                document.redo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), saved);
            }
        }
    }
}

#[test]
fn singleton_spacing_normalization_preserves_authored_source_and_real_loose_lists() {
    for (source, tight, first) in [
        ("- One\n", true, "One"),
        ("- One\n\n- Two\n", false, "One"),
        ("- One\n\n  Two\n", false, "One"),
        ("- One[^n]\n\n  [^n]: note.\n", true, "One[^n]"),
        ("7. One[^n]\n\n   [^n]: note.\n", true, "One[^n]"),
        ("- [x] One[^n]\n\n  [^n]: note.\n", true, "One[^n]"),
    ] {
        for newline in ["\n", "\r\n"] {
            let source = source.replace('\n', newline);
            let mut document = Document::from_markdown(source.clone()).unwrap();
            let before = document.snapshot();
            let BlockNode::List(list) = before.blocks().get(0).unwrap().as_ref() else {
                panic!("list");
            };
            assert_eq!(list.tight, tight, "{source}");
            assert_eq!(before.serialize().unwrap(), source);
            assert!(before.dirty_node_ids().is_empty());
            assert!(matches!(
                document.undo(),
                Err(document_core::DocumentError::NothingToUndo)
            ));
            edit(&mut document, first, "New ");
            let saved = document.snapshot().serialize().unwrap();
            assert_reopens(&document, &saved);
            let BlockNode::List(edited) = document.snapshot().node(list.id).unwrap().clone() else {
                panic!("list");
            };
            assert_eq!(edited.tight, tight);
            assert!(document.rebase_source(document.snapshot().prepare_source_rebase().unwrap()));
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), source);
            assert_eq!(document.snapshot().selection(), before.selection());
            document.redo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), saved);
        }
    }
}

#[test]
fn moving_reference_owners_keeps_original_definition_precedence() {
    for source in [
        "[ref]: /first \"First\"\n\nOne\n\n[REF]: /second \"Second\"\n\nTwo\n\nOutside [link][ref].\n",
        "> One\n>\n> [ref]: /first \"First\"\n\n> Two\n>\n> [REF]: /second \"Second\"\n\nOutside [link][ref].\n",
    ] {
        let mut document = Document::from_markdown(source).unwrap();
        let before = document.snapshot();
        let outside = before
            .blocks()
            .iter()
            .find_map(|b| leaf(b, "Outside link."))
            .unwrap();
        let runs = before
            .node(outside)
            .unwrap()
            .text()
            .unwrap()
            .runs()
            .to_vec();
        document
            .apply(EditCommand::MoveBlock { from: 0, to: 2 })
            .unwrap();
        let saved = document.snapshot().serialize().unwrap();
        let reopened = Document::from_markdown(saved.clone()).unwrap().snapshot();
        let outside = reopened
            .blocks()
            .iter()
            .find_map(|b| leaf(b, "Outside link."))
            .unwrap();
        assert_eq!(
            reopened.node(outside).unwrap().text().unwrap().runs(),
            runs,
            "{saved}"
        );
        assert_eq!(saved.matches("[ref]:").count(), 1);
        assert_eq!(saved.matches("[REF]:").count(), 1);
        assert_reopens(&document, &saved);
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        document.redo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), saved);
    }
}

#[test]
fn repeated_reference_owner_moves_rebase_and_undo_without_retargeting_links() {
    let orders = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    for body in [
        "[ref]: /first \"First\"\n\nOne\n\n[REF]: /second \"Second\"\n\nTwo\n\nOutside [link][ref].",
        "> One\n>\n> [ref]: /first \"First\"\n\n> Two\n>\n> [REF]: /second \"Second\"\n\nOutside [link][ref].",
    ] {
        for newline in ["\n", "\r\n"] {
            for order in orders {
                let source = format!("Stable\n======\n\n{body}").replace('\n', newline);
                let mut document = Document::from_markdown(source.clone()).unwrap();
                let original = document.snapshot();
                let ids = original
                    .blocks()
                    .iter()
                    .skip(1)
                    .map(|b| b.id())
                    .collect::<Vec<_>>();
                assert_eq!(ids.len(), 3);
                let outside = original
                    .blocks()
                    .iter()
                    .find_map(|b| leaf(b, "Outside link."))
                    .unwrap();
                let expected_runs = original
                    .node(outside)
                    .unwrap()
                    .text()
                    .unwrap()
                    .runs()
                    .to_vec();
                let mut history = Vec::new();
                for (target, ordinal) in order.into_iter().enumerate() {
                    let snapshot = document.snapshot();
                    let from = snapshot
                        .blocks()
                        .iter()
                        .position(|b| b.id() == ids[ordinal])
                        .unwrap();
                    let to = target + 1;
                    if from == to {
                        continue;
                    }
                    let before = snapshot.serialize().unwrap();
                    document.apply(EditCommand::MoveBlock { from, to }).unwrap();
                    let saved = document.snapshot().serialize().unwrap();
                    assert!(
                        saved.starts_with(&format!("Stable{newline}======{newline}")),
                        "{saved}"
                    );
                    assert_eq!(saved.matches("[ref]:").count(), 1, "{saved}");
                    assert_eq!(saved.matches("[REF]:").count(), 1, "{saved}");
                    let reopened = Document::from_markdown(saved.clone()).unwrap().snapshot();
                    let id = reopened
                        .blocks()
                        .iter()
                        .find_map(|b| leaf(b, "Outside link."))
                        .expect("outside link survives move");
                    assert_eq!(
                        reopened.node(id).unwrap().text().unwrap().runs(),
                        expected_runs,
                        "{saved}"
                    );
                    assert_reopens(&document, &saved);
                    assert!(
                        document
                            .rebase_source(document.snapshot().prepare_source_rebase().unwrap()),
                        "{saved}"
                    );
                    history.push((before, saved));
                }
                for (before, _) in history.iter().rev() {
                    document.undo().unwrap();
                    assert_eq!(document.snapshot().serialize().unwrap(), *before);
                }
                assert_eq!(document.snapshot().serialize().unwrap(), source);
                assert_eq!(document.snapshot().selection(), original.selection());
                for (_, after) in &history {
                    document.redo().unwrap();
                    assert_eq!(document.snapshot().serialize().unwrap(), *after);
                }
            }
        }
    }
}

#[test]
fn all_reference_owners_can_move_before_any_source_rebase() {
    let source = "> One\n>\n> [ref]: /first\n\n> Two\n>\n> [REF]: /second\n\nOutside [link][ref].";
    let mut document = Document::from_markdown(source).unwrap();
    let original = document.snapshot();
    let id = original
        .blocks()
        .iter()
        .find_map(|b| leaf(b, "Outside link."))
        .unwrap();
    let runs = original.node(id).unwrap().text().unwrap().runs().to_vec();
    for _ in 0..3 {
        document
            .apply(EditCommand::MoveBlock { from: 0, to: 2 })
            .unwrap();
        let saved = document.snapshot().serialize().unwrap();
        assert_eq!(saved.matches("[ref]:").count(), 1, "{saved}");
        assert_eq!(saved.matches("[REF]:").count(), 1, "{saved}");
        let reopened = Document::from_markdown(saved.clone()).unwrap().snapshot();
        let id = reopened
            .blocks()
            .iter()
            .find_map(|b| leaf(b, "Outside link."))
            .expect("link remains a separate paragraph");
        assert_eq!(reopened.node(id).unwrap().text().unwrap().runs(), runs);
        assert_reopens(&document, &saved);
    }
    assert!(document.rebase_source(document.snapshot().prepare_source_rebase().unwrap()));
    for _ in 0..3 {
        document.undo().unwrap();
    }
    assert_eq!(document.snapshot().serialize().unwrap(), source);
}

#[test]
fn nested_footnote_continuation_boundaries_survive_unrelated_structural_edits() {
    for prefix in [
        "> Ref[^n]\n>\n> [^n]: note\n\n    continuation\n\n",
        "> Ref[^n]\n>\n> [^n]: note\n    continuation\n\n",
        "- Ref[^n]\n\n  [^n]: note\n\n    continuation\n\n",
        "- Ref[^n]\n\n  [^n]: note\n    continuation\n\n",
        "> > Ref[^n]\n> >\n> > [^n]: note\n>\n>     continuation\n\n",
        "> - Ref[^n]\n>\n>   [^n]: note\n>\n>       continuation\n\n",
    ] {
        for newline in ["\n", "\r\n"] {
            let prefix = prefix.replace('\n', newline);
            let source = format!("{prefix}After{newline}");
            let mut document = Document::from_markdown(source.clone()).unwrap();
            let id = document
                .snapshot()
                .blocks()
                .iter()
                .find_map(|b| leaf(b, "After"))
                .unwrap();
            document
                .apply(EditCommand::SetSelection(Selection::Text(
                    TextSelection::caret(DocumentPosition::new(id, 2, Affinity::Downstream)),
                )))
                .unwrap();
            document.apply(EditCommand::SplitSelection).unwrap();
            let saved = document.snapshot().serialize().unwrap();
            assert_eq!(saved.matches("[^n]:").count(), 1, "{source:?} -> {saved:?}");
            assert!(
                saved.starts_with(&prefix),
                "untouched context changed: {source:?} -> {saved:?}"
            );
            assert_reopens(&document, &saved);
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), source);
            document.redo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), saved);
        }
    }
}

#[test]
fn empty_paragraphs_remain_live_carets_without_becoming_persisted_blocks() {
    for newline in ["\n", "\r\n"] {
        let source = format!("One{newline}{newline}Two{newline}");
        let mut document = Document::from_markdown(source.clone()).unwrap();
        let first = document.snapshot().blocks().get(0).unwrap().id();
        let original_selection = Selection::Text(TextSelection::caret(DocumentPosition::new(
            first,
            1,
            Affinity::Downstream,
        )));
        document
            .apply(EditCommand::SetSelection(original_selection.clone()))
            .unwrap();
        for _ in 0..4 {
            document.apply(EditCommand::SplitSelection).unwrap();
        }
        let before_save = document.snapshot();
        let empty = before_save
            .blocks()
            .iter()
            .find(|b| matches!(b.as_ref(), BlockNode::Paragraph(p) if p.content.is_empty()))
            .expect("live empty paragraph")
            .id();
        let saved = before_save.serialize().unwrap();
        let reopened = Document::from_markdown(saved.clone()).unwrap().snapshot();
        assert_eq!(
            reopened
                .blocks()
                .iter()
                .map(|b| b.plain_text())
                .collect::<Vec<_>>(),
            ["O", "ne", "Two"]
        );
        assert!(!saved.contains("<p"), "no invented persisted placeholder");
        assert!(
            !document.rebase_source(before_save.prepare_source_rebase().unwrap()),
            "do not attach parsed ranges to unmatched caret nodes"
        );
        assert_eq!(document.snapshot().selection(), before_save.selection());
        assert_eq!(document.snapshot().revision(), before_save.revision());
        assert!(document.snapshot().node(empty).is_some());
        document
            .apply(EditCommand::SetSelection(Selection::Text(
                TextSelection::caret(DocumentPosition::new(empty, 0, Affinity::Downstream)),
            )))
            .unwrap();
        document
            .apply(EditCommand::ReplaceSelection {
                text: "Inserted".into(),
                typing: false,
            })
            .unwrap();
        let typed = document.snapshot().serialize().unwrap();
        let reopened = Document::from_markdown(typed.clone()).unwrap().snapshot();
        assert_eq!(
            reopened
                .blocks()
                .iter()
                .map(|b| b.plain_text())
                .collect::<Vec<_>>(),
            ["O", "Inserted", "ne", "Two"]
        );
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), saved);
        for _ in 0..4 {
            document.undo().unwrap();
        }
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        assert_eq!(document.snapshot().selection(), &original_selection);
        for _ in 0..5 {
            document.redo().unwrap();
        }
        assert_eq!(document.snapshot().serialize().unwrap(), typed);
    }
}

#[test]
fn all_empty_document_reopens_with_one_caret_and_can_be_typed_after_save() {
    let mut document = Document::empty();
    for _ in 0..3 {
        document.apply(EditCommand::SplitSelection).unwrap();
    }
    let before = document.snapshot();
    let saved = before.serialize().unwrap();
    assert!(saved.trim().is_empty());
    let reopened = Document::from_markdown(saved.clone()).unwrap().snapshot();
    assert_eq!(reopened.blocks().len(), 1);
    assert!(reopened.blocks().get(0).unwrap().plain_text().is_empty());
    assert!(!document.rebase_source(before.prepare_source_rebase().unwrap()));
    document
        .apply(EditCommand::ReplaceSelection {
            text: "Written".into(),
            typing: false,
        })
        .unwrap();
    let typed = document.snapshot().serialize().unwrap();
    let reopened = Document::from_markdown(typed).unwrap().snapshot();
    assert_eq!(reopened.blocks().len(), 1);
    assert_eq!(reopened.blocks().get(0).unwrap().plain_text(), "Written");
    document.undo().unwrap();
    assert_eq!(document.snapshot().serialize().unwrap(), saved);
    for _ in 0..3 {
        document.undo().unwrap();
    }
    assert_eq!(document.snapshot().serialize().unwrap(), "");
}

#[test]
fn cross_container_replacement_retains_outside_reference_meaning() {
    for body in [
        "> Start\n>\n> [ref]: /local \"Title\"\n\nFinish\n",
        "> [!NOTE]\n> Start\n>\n> [ref]: /local \"Title\"\n\nFinish\n",
        "- Start\n\n  [ref]: /local \"Title\"\n\nFinish\n",
        "> - Start\n>\n>   [ref]: /local \"Title\"\n\nFinish\n",
        "<table><tr><td><p>Start</p></td></tr></table>\n\n[ref]: /local \"Title\"\n\nFinish\n",
    ] {
        for newline in ["\n", "\r\n"] {
            for reverse in [false, true] {
                for offset in [0, 2] {
                    for replacement in ["", "New", "**New**\n\nParagraph"] {
                        let source = format!(
                            "Stable\n======\n\n{body}\nOutside [link][ref].\n\n~~~rust\ncode\n~~~\n"
                        )
                        .replace('\n', newline);
                        let mut document = Document::from_markdown(source.clone()).unwrap();
                        let original = document.snapshot();
                        let a = original
                            .blocks()
                            .iter()
                            .find_map(|b| leaf(b, "Start"))
                            .unwrap();
                        let b = original
                            .blocks()
                            .iter()
                            .find_map(|b| leaf(b, "Finish"))
                            .unwrap();
                        let outside = original
                            .blocks()
                            .iter()
                            .find_map(|b| leaf(b, "Outside link."))
                            .unwrap();
                        let runs = original
                            .node(outside)
                            .unwrap()
                            .text()
                            .unwrap()
                            .runs()
                            .to_vec();
                        let mut range = TextSelection {
                            anchor: DocumentPosition::new(a, offset, Affinity::Downstream),
                            head: DocumentPosition::new(b, 3, Affinity::Downstream),
                        };
                        if reverse {
                            std::mem::swap(&mut range.anchor, &mut range.head);
                        }
                        let selection = Selection::Text(range);
                        document
                            .apply(EditCommand::SetSelection(selection.clone()))
                            .unwrap();
                        if replacement.contains("**") {
                            document
                                .apply(EditCommand::PasteMarkdown {
                                    markdown: replacement.into(),
                                })
                                .unwrap();
                        } else {
                            document
                                .apply(EditCommand::ReplaceSelection {
                                    text: replacement.into(),
                                    typing: false,
                                })
                                .unwrap();
                        }
                        let saved = document.snapshot().serialize().unwrap();
                        assert_eq!(
                            saved.matches("[ref]:").count(),
                            1,
                            "{body:?}, {offset}, {replacement:?}: {saved}"
                        );
                        assert!(saved.starts_with(&format!("Stable{newline}======{newline}")));
                        assert!(
                            saved.ends_with(&format!("~~~rust{newline}code{newline}~~~{newline}"))
                        );
                        assert!(saved.contains("Outside [link][ref]."));
                        let reopened = Document::from_markdown(saved.clone()).unwrap().snapshot();
                        let outside = reopened
                            .blocks()
                            .iter()
                            .find_map(|b| leaf(b, "Outside link."))
                            .expect("outside reference still resolves");
                        assert_eq!(reopened.node(outside).unwrap().text().unwrap().runs(), runs);
                        assert_reopens(&document, &saved);
                        document.undo().unwrap();
                        assert_eq!(document.snapshot().serialize().unwrap(), source);
                        assert_eq!(document.snapshot().selection(), &selection);
                        document.redo().unwrap();
                        assert_eq!(document.snapshot().serialize().unwrap(), saved);
                    }
                }
            }
        }
    }
}

#[test]
fn empty_quotes_and_alerts_own_editable_source_preserving_carets() {
    for source in [
        ">\n",
        "> [!NOTE]\n",
        "> >\n",
        "> [ref]: /local\n\nOutside [link][ref].\n",
    ] {
        let mut document = Document::from_markdown(source).unwrap();
        let before = document.snapshot();
        let root = before.blocks().get(0).unwrap();
        let id = leaf(root, "").expect("the container owns an empty caret host");
        assert_eq!(before.serialize().unwrap(), source);
        assert!(before.dirty_node_ids().is_empty());
        assert!(
            matches!(before.selection(), Selection::Text(selection) if selection.anchor.node_id == id)
        );
        document
            .apply(EditCommand::ReplaceSelection {
                text: "Written".into(),
                typing: false,
            })
            .unwrap();
        let saved = document.snapshot().serialize().unwrap();
        assert_reopens(&document, &saved);
        assert!(document.rebase_source(document.snapshot().prepare_source_rebase().unwrap()));
        let reopened = Document::from_markdown(saved.clone()).unwrap().snapshot();
        assert!(leaf(reopened.blocks().get(0).unwrap(), "Written").is_some());
        if source.contains("[ref]") {
            assert_eq!(saved.matches("[ref]:").count(), 1);
        }
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        assert_eq!(document.snapshot().selection(), before.selection());
        document.redo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), saved);
    }
}

#[test]
fn html_table_code_blocks_keep_blank_lines_after_presentation_edits() {
    for newline in ["\n", "\r\n"] {
        let source = "<table><tr><td><pre><code class='language-rust'>First&#10;&#10;Third&#10;</code></pre><p>Caption</p></td></tr></table>\n".replace('\n', newline);
        let mut document = Document::from_markdown(source.as_str()).unwrap();
        let before = document.snapshot();
        document
            .apply(EditCommand::SetTableBorder {
                table_id: before.blocks().get(0).unwrap().id(),
                border: document_core::TableBorder::Dotted,
            })
            .unwrap();
        let saved = document.snapshot().serialize().unwrap();
        assert_reopens(&document, &saved);
        assert!(document.rebase_source(document.snapshot().prepare_source_rebase().unwrap()));
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        document.redo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), saved);
    }
}

#[test]
fn html_required_cells_preserve_edited_breaks_and_edge_whitespace() {
    for value in [
        "First\nSecond",
        "First  \nSecond",
        " leading ",
        "\tTabbed\t",
        "First\n\nThird",
        "First\r\nSecond",
        "  ",
        "<&> | ` Ω\tend ",
    ] {
        for newline in ["\n", "\r\n"] {
            for format in [
                None,
                Some(document_core::InlineFormat::Bold),
                Some(document_core::InlineFormat::Italic),
                Some(document_core::InlineFormat::Strikethrough),
                Some(document_core::InlineFormat::Code),
                Some(document_core::InlineFormat::Link("/destination".into())),
            ] {
                let source = "<table><tr><td><p>Value</p><p>Caption</p></td></tr></table>\n"
                    .replace('\n', newline);
                let mut document = Document::from_markdown(source.as_str()).unwrap();
                let before = document.snapshot();
                let table_id = before.blocks().get(0).unwrap().id();
                let id = leaf(before.blocks().get(0).unwrap(), "Value").unwrap();
                document
                    .apply(EditCommand::ReplaceText {
                        node_id: id,
                        range: 0..5,
                        text: value.into(),
                        selection_after: None,
                        typing: false,
                    })
                    .unwrap();
                let styled = format.is_some();
                if let Some(format) = format {
                    document
                        .apply(EditCommand::ToggleInline {
                            node_id: id,
                            range: 0..value.len(),
                            format,
                        })
                        .unwrap();
                }
                document
                    .apply(EditCommand::SetTableBorder {
                        table_id,
                        border: document_core::TableBorder::Dotted,
                    })
                    .unwrap();
                let saved = document.snapshot().serialize().unwrap();
                assert_reopens(&document, &saved);
                assert!(
                    document.rebase_source(document.snapshot().prepare_source_rebase().unwrap()),
                    "{value:?}: {saved}"
                );
                document.undo().unwrap();
                if styled {
                    document.undo().unwrap();
                }
                document.undo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), source);
                assert_eq!(document.snapshot().selection(), before.selection());
                document.redo().unwrap();
                if styled {
                    document.redo().unwrap();
                }
                document.redo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), saved);
            }
        }
    }
}

#[test]
fn html_table_mixed_task_states_survive_presentation_edits() {
    let source = "<table><tr><td><ul><li><input type='checkbox' checked disabled><p><strong>Done</strong></p><p>Second paragraph</p></li><li><p>Plain</p></li><li><p><input type='checkbox' disabled>Todo <a href='/destination'>link</a></p><ul><li><input type='checkbox' checked disabled><p>Nested</p></li></ul></li></ul><p>Caption</p></td></tr></table>\n";
    for newline in ["\n", "\r\n"] {
        let source = source.replace('\n', newline);
        let mut document = Document::from_markdown(source.as_str()).unwrap();
        let before = document.snapshot();
        let BlockNode::Table(table) = before.blocks().get(0).unwrap().as_ref() else {
            panic!("table")
        };
        let BlockNode::List(list) = table.rows[0].cells[0].blocks.get(0).unwrap().as_ref() else {
            panic!("list")
        };
        assert_eq!(
            list.items.iter().map(|i| i.checked).collect::<Vec<_>>(),
            [Some(true), None, Some(false)]
        );
        document
            .apply(EditCommand::SetTableBorder {
                table_id: table.id,
                border: document_core::TableBorder::Dotted,
            })
            .unwrap();
        let saved = document.snapshot().serialize().unwrap();
        assert_reopens(&document, &saved);
        assert!(document.rebase_source(document.snapshot().prepare_source_rebase().unwrap()));
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        assert_eq!(document.snapshot().selection(), before.selection());
        document.redo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), saved);
    }
}

#[test]
fn singleton_nonparagraph_lists_reopen_after_their_notes_move_apart() {
    for source in [
        "- # Heading[^n]\n\n  [^n]: Note\n\nAfter\n",
        "- ```rust\n  code\n  ```\n\n  [^n]: Note\n\nOutside[^n]\n",
        "1. ```rust\n   code\n   ```\n\n   [^n]: Note\n\nOutside[^n]\n",
        "9999. ```rust\n      code\n      ```\n\n      [^n]: Note\n\nOutside[^n]\n",
        "- > Body[^n]\n\n  [^n]: Note\n\nAfter\n",
    ] {
        let mut document = Document::from_markdown(source).unwrap();
        let before = document.snapshot();
        assert_eq!(before.serialize().unwrap(), source);
        document
            .apply(EditCommand::MoveBlock {
                from: 0,
                to: before.blocks().len() - 1,
            })
            .unwrap();
        let saved = document.snapshot().serialize().unwrap();
        assert_eq!(saved.matches("[^n]:").count(), 1);
        assert_reopens(&document, &saved);
        assert!(document.rebase_source(document.snapshot().prepare_source_rebase().unwrap()));
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        assert_eq!(document.snapshot().selection(), before.selection());
        document.redo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), saved);
    }
}
