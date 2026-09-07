//! Resolve transient preview addresses on a private transactional snapshot.
//! Shared by read-only copy and atomic conversion-on-edit.
use super::*;
use crate::{PreviewPosition, PreviewSelection};

pub(super) fn resolve(
    state: &mut SnapshotState,
    selection: &PreviewSelection,
) -> Result<bool, DocumentError> {
    if state.revision != selection.revision {
        return Err(DocumentError::Html(
            "The document changed; choose the text again".into(),
        ));
    }
    let endpoints = [&selection.anchor, &selection.head];
    let mut resolved = [None, None];
    // Validate both addresses before converting anything. All work still takes
    // place on the caller's private snapshot, including later range errors.
    for (index, endpoint) in endpoints.iter().enumerate() {
        match endpoint {
            PreviewPosition::Document(position) => {
                validate_selection_in_blocks(
                    &state.blocks,
                    &Selection::Text(TextSelection::caret(*position)),
                )?;
                resolved[index] = Some(*position);
            }
            PreviewPosition::Html {
                node_id,
                expected_source,
                ..
            } => {
                if !matches!(find_node(&state.blocks, *node_id),
                    Some(BlockNode::PreservedSource { source, .. }) if source == expected_source)
                {
                    return Err(DocumentError::Html(
                        "The HTML changed; choose the text again".into(),
                    ));
                }
            }
        }
    }
    let mut order = Vec::new();
    collect_blocks(&state.blocks, &mut order);
    let index_of = |endpoint: &PreviewPosition| {
        order
            .iter()
            .position(|(id, _)| *id == endpoint.node_id())
            .ok_or(PositionError::UnknownNode(endpoint.node_id()))
    };
    let anchor_index = index_of(&selection.anchor)?;
    let head_index = index_of(&selection.head)?;
    let mut converted = false;
    for (node_id, source) in &order[anchor_index.min(head_index)..=anchor_index.max(head_index)] {
        let Some(source) = source else {
            continue;
        };
        let target = |endpoint: &PreviewPosition| match endpoint {
            PreviewPosition::Html {
                node_id: owner,
                position,
                ..
            } if owner == node_id => Some(*position),
            _ => None,
        };
        let anchor = target(&selection.anchor);
        let head = target(&selection.head);
        let targets = anchor.or(head).map(|first| {
            (
                source.as_ref(),
                anchor.unwrap_or(first),
                head.unwrap_or(first),
            )
        });
        let range = convert_html_to_markdown(state, *node_id, targets)?;
        if anchor.is_some() {
            resolved[0] = Some(range.anchor);
        }
        if head.is_some() {
            resolved[1] = Some(range.head);
        }
        converted = true;
    }
    let [Some(anchor), Some(head)] = resolved else {
        return Err(DocumentError::Html(
            "The preview range could not be resolved".into(),
        ));
    };
    state.selection = Selection::Text(TextSelection { anchor, head });
    validate_selection_in_blocks(&state.blocks, &state.selection)?;
    retire_transient_caret(state);
    validate_tree(&state.blocks)?;
    Ok(converted)
}

/// One source-order traversal; don't repeatedly search the tree for every
/// selected fragment. Arc source handles survive each local replacement.
fn collect_blocks(blocks: &BlockSequence, output: &mut Vec<(NodeId, Option<Arc<str>>)>) {
    for block in blocks {
        output.push((
            block.id(),
            match block.as_ref() {
                BlockNode::PreservedSource { source, .. } => Some(source.clone()),
                _ => None,
            },
        ));
        match block.as_ref() {
            BlockNode::List(list) => {
                for item in list.items.iter() {
                    collect_blocks(&item.blocks, output);
                }
            }
            BlockNode::BlockQuote { blocks, .. }
            | BlockNode::Alert { blocks, .. }
            | BlockNode::FootnoteDefinition { blocks, .. } => collect_blocks(blocks, output),
            BlockNode::Table(table) => {
                for row in table.rows.iter() {
                    for cell in row.cells.iter() {
                        collect_blocks(&cell.blocks, output);
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "Before\n\n<div><p>Alpha <strong>café</strong></p></div>\n\nMiddle\n\n<div><p>Beta <em>tail</em></p></div>\n\nAfter\n";

    fn endpoint(snapshot: &DocumentSnapshot, index: usize, byte_offset: usize) -> PreviewPosition {
        let node = snapshot.blocks().get(index).unwrap();
        match node.as_ref() {
            BlockNode::PreservedSource { source, .. } => PreviewPosition::Html {
                node_id: node.id(),
                expected_source: source.clone(),
                position: crate::HtmlTextPosition {
                    text_node: 0,
                    byte_offset,
                },
            },
            _ => PreviewPosition::Document(DocumentPosition::new(
                node.id(),
                byte_offset,
                crate::Affinity::Downstream,
            )),
        }
    }

    #[test]
    fn ordinary_cross_block_composition_matches_preview_composition() {
        for preview in [false, true] {
            for reverse in [false, true] {
                let mut document = Document::from_markdown(SOURCE).unwrap();
                let before = document.snapshot();
                let a = DocumentPosition::new(
                    before.blocks().get(0).unwrap().id(),
                    2,
                    crate::Affinity::Downstream,
                );
                let b = DocumentPosition::new(
                    before.blocks().get(4).unwrap().id(),
                    2,
                    crate::Affinity::Upstream,
                );
                let range = TextSelection {
                    anchor: if reverse { b } else { a },
                    head: if reverse { a } else { b },
                };
                if preview {
                    document
                        .begin_preview_composition(&PreviewSelection {
                            revision: before.revision(),
                            anchor: PreviewPosition::Document(range.anchor),
                            head: PreviewPosition::Document(range.head),
                        })
                        .unwrap();
                } else {
                    document.begin_composition(range).unwrap();
                }
                assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
                for text in ["仮", "確定"] {
                    let snapshot = document.update_composition(text.into()).unwrap();
                    assert_eq!(snapshot.serialize().unwrap(), format!("Be{text}ter\n"));
                    assert_eq!(snapshot.blocks().len(), 1);
                }
                document.commit_composition().unwrap();
                document.undo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
                assert!(matches!(document.undo(), Err(DocumentError::NothingToUndo)));
            }
        }
    }

    #[test]
    fn preview_composition_restarts_from_baseline_and_is_one_undoable_edit() {
        for reverse in [false, true] {
            for commit in [false, true] {
                let mut document = Document::from_markdown(SOURCE).unwrap();
                let before = document.snapshot();
                let anchor = PreviewPosition::Html {
                    node_id: before.blocks().get(1).unwrap().id(),
                    expected_source: match before.blocks().get(1).unwrap().as_ref() {
                        BlockNode::PreservedSource { source, .. } => source.clone(),
                        _ => unreachable!(),
                    },
                    position: crate::HtmlTextPosition {
                        text_node: 0,
                        byte_offset: 6,
                    },
                };
                let head = endpoint(&before, 3, 4);
                let selection = PreviewSelection {
                    revision: before.revision(),
                    anchor: if reverse {
                        head.clone()
                    } else {
                        anchor.clone()
                    },
                    head: if reverse { anchor } else { head },
                };
                document.begin_preview_composition(&selection).unwrap();
                assert_eq!(document.snapshot().revision(), before.revision());
                assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
                assert!(matches!(
                    document.begin_preview_composition(&selection),
                    Err(DocumentError::CompositionAlreadyActive)
                ));
                let mut caret_node = None;
                for text in ["仮", "😀", "確定"] {
                    let after = document.update_composition(text.into()).unwrap();
                    assert_eq!(
                        after.serialize().unwrap(),
                        format!("Before\n\nAlpha {text} *tail*\n\nAfter\n")
                    );
                    let Selection::Text(caret) = after.selection() else {
                        panic!("caret")
                    };
                    assert_eq!(caret.head.text_offset, 6 + text.len());
                    assert_eq!(
                        *caret_node.get_or_insert(caret.head.node_id),
                        caret.head.node_id
                    );
                    assert!(Arc::ptr_eq(
                        before.blocks().get(0).unwrap(),
                        after.blocks().get(0).unwrap()
                    ));
                    assert!(Arc::ptr_eq(
                        before.blocks().get(4).unwrap(),
                        after.blocks().get(2).unwrap()
                    ));
                }
                if commit {
                    let after = document.commit_composition().unwrap().serialize().unwrap();
                    document.undo().unwrap();
                    assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
                    assert!(matches!(document.undo(), Err(DocumentError::NothingToUndo)));
                    document.redo().unwrap();
                    assert_eq!(document.snapshot().serialize().unwrap(), after);
                } else {
                    document.cancel_composition().unwrap();
                    assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
                    assert_eq!(document.snapshot().selection(), before.selection());
                    assert!(matches!(document.undo(), Err(DocumentError::NothingToUndo)));
                    assert!(
                        document.begin_preview_composition(&selection).is_err(),
                        "cancelled revisions must not accept stale addresses"
                    );
                    assert!(!document.composition_active());
                }
            }
        }
    }

    #[test]
    fn cross_preview_copy_and_edit_are_source_ordered_and_atomic() {
        for reverse in [false, true] {
            let mut document = Document::from_markdown(SOURCE).unwrap();
            let before = document.snapshot();
            let mut selection = PreviewSelection {
                revision: before.revision(),
                anchor: endpoint(&before, 1, 6),
                head: endpoint(&before, 3, 4),
            };
            if reverse {
                std::mem::swap(&mut selection.anchor, &mut selection.head);
            }
            let payload = before
                .preview_clipboard_payload(&selection)
                .unwrap()
                .unwrap();
            assert_eq!(payload.plain_text.as_deref(), Some("café\nMiddle\nBeta"));
            assert!(
                crate::RichClipboard::from_json(payload.rich_json.as_deref().unwrap())
                    .unwrap()
                    .markdown
                    .contains("**café**")
            );
            assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
            assert_eq!(document.snapshot().revision(), before.revision());
            assert_eq!(document.snapshot().selection(), before.selection());
            assert!(matches!(document.undo(), Err(DocumentError::NothingToUndo)));

            document
                .apply(EditCommand::EditPreviewSelection {
                    selection,
                    edit: crate::HtmlTextEdit::Replace("X".into()),
                })
                .unwrap();
            let edited = document.snapshot();
            assert_eq!(edited.blocks().get(1).unwrap().plain_text(), "Alpha X tail");
            assert_eq!(edited.blocks().len(), 3);
            assert_eq!(
                edited.serialize().unwrap(),
                "Before\n\nAlpha X *tail*\n\nAfter\n",
                "replacement must not retain separators of deleted blocks"
            );
            assert!(Arc::ptr_eq(
                before.blocks().get(0).unwrap(),
                edited.blocks().get(0).unwrap()
            ));
            assert!(Arc::ptr_eq(
                before.blocks().get(4).unwrap(),
                edited.blocks().get(2).unwrap()
            ));
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
            assert_eq!(document.snapshot().selection(), before.selection());
            assert!(matches!(document.undo(), Err(DocumentError::NothingToUndo)));
            document.redo().unwrap();
            assert_eq!(
                document.snapshot().serialize().unwrap(),
                edited.serialize().unwrap()
            );
        }
    }

    #[test]
    fn mixed_preview_edits_match_explicit_conversion_and_normal_commands() {
        // Both mixed directions, HTML in the interior, and two endpoints in
        // the same fragment. Include reverse selections and every edit kind.
        for (start, end) in [(0, 3), (1, 4), (0, 4), (1, 1)] {
            for reverse in [false, true] {
                for edit in [
                    crate::HtmlTextEdit::Replace("X".into()),
                    crate::HtmlTextEdit::Replace(String::new()),
                    crate::HtmlTextEdit::PasteMarkdown("**new**".into()),
                    crate::HtmlTextEdit::Format(crate::InlineFormat::Italic),
                    crate::HtmlTextEdit::Link(Some("https://example.test".into())),
                    crate::HtmlTextEdit::Split,
                ] {
                    let mut atomic = Document::from_markdown(SOURCE).unwrap();
                    let mut reference = Document::from_markdown(SOURCE).unwrap();
                    let before = atomic.snapshot();
                    let mut selection = PreviewSelection {
                        revision: before.revision(),
                        anchor: endpoint(&before, start, 2),
                        head: endpoint(&before, end, 4),
                    };
                    if reverse {
                        std::mem::swap(&mut selection.anchor, &mut selection.head);
                    }
                    let mut converted = HashMap::new();
                    for index in start..=end {
                        let node = before.blocks().get(index).unwrap();
                        if matches!(node.as_ref(), BlockNode::PreservedSource { .. }) {
                            reference
                                .apply(EditCommand::ConvertHtmlToMarkdown { node_id: node.id() })
                                .unwrap();
                            let Selection::Text(range) = reference.snapshot().selection().clone()
                            else {
                                panic!("text");
                            };
                            converted.insert(node.id(), range.head.node_id);
                        }
                    }
                    let map = |position: &PreviewPosition| match position {
                        PreviewPosition::Document(position) => *position,
                        PreviewPosition::Html {
                            node_id, position, ..
                        } => DocumentPosition::new(
                            converted[node_id],
                            position.byte_offset,
                            crate::Affinity::Downstream,
                        ),
                    };
                    reference
                        .apply(EditCommand::SetSelection(Selection::Text(TextSelection {
                            anchor: map(&selection.anchor),
                            head: map(&selection.head),
                        })))
                        .unwrap();
                    assert_eq!(
                        before.preview_clipboard_payload(&selection).unwrap(),
                        reference.snapshot().clipboard_payload().unwrap()
                    );
                    reference.apply(edit.clone().into_command()).unwrap_or_else(|error| panic!("reference {start}..{end}, reverse={reverse}, edit={edit:?}: {error:?}"));
                    atomic
                        .apply(EditCommand::EditPreviewSelection { selection, edit })
                        .unwrap();
                    assert_eq!(
                        atomic.snapshot().serialize().unwrap(),
                        reference.snapshot().serialize().unwrap()
                    );
                    atomic.undo().unwrap();
                    assert_eq!(atomic.snapshot().serialize().unwrap(), SOURCE);
                    assert!(matches!(atomic.undo(), Err(DocumentError::NothingToUndo)));
                }
            }
        }
    }

    #[test]
    fn preview_range_errors_and_empty_edits_never_publish_partial_conversion() {
        let mut document = Document::from_markdown(SOURCE).unwrap();
        let before = document.snapshot();
        let valid = PreviewSelection {
            revision: before.revision(),
            anchor: endpoint(&before, 1, 2),
            head: endpoint(&before, 3, 4),
        };
        let mut invalid = Vec::new();
        let mut stale = valid.clone();
        stale.revision = Revision(stale.revision.0 + 1);
        invalid.push(stale);
        for position in [
            PreviewPosition::Html {
                node_id: valid.head.node_id(),
                expected_source: "stale".into(),
                position: crate::HtmlTextPosition {
                    text_node: 0,
                    byte_offset: 0,
                },
            },
            PreviewPosition::Document(DocumentPosition::new(
                valid.head.node_id(),
                0,
                crate::Affinity::Downstream,
            )),
            PreviewPosition::Document(DocumentPosition::new(
                NodeId::new_unchecked(u64::MAX),
                0,
                crate::Affinity::Downstream,
            )),
            endpoint(&before, 0, usize::MAX),
        ] {
            invalid.push(PreviewSelection {
                head: position,
                ..valid.clone()
            });
        }
        // Validate the second endpoint only after the first fragment has been
        // converted on the private working state: neither error may leak it.
        for (text_node, byte_offset) in [(99, 0), (0, usize::MAX)] {
            let mut selection = valid.clone();
            let PreviewPosition::Html { position, .. } = &mut selection.head else {
                unreachable!()
            };
            *position = crate::HtmlTextPosition {
                text_node,
                byte_offset,
            };
            invalid.push(selection);
        }
        invalid.push(PreviewSelection {
            anchor: endpoint(&before, 1, 10),
            ..valid.clone()
        }); // inside é
        for selection in invalid {
            assert!(before.preview_clipboard_payload(&selection).is_err());
            assert!(
                document
                    .apply(EditCommand::EditPreviewSelection {
                        selection,
                        edit: crate::HtmlTextEdit::Replace("X".into()),
                    })
                    .is_err()
            );
            assert!(Arc::ptr_eq(&document.snapshot().0, &before.0));
            assert!(matches!(document.undo(), Err(DocumentError::NothingToUndo)));
        }
        let caret = PreviewSelection {
            head: valid.anchor.clone(),
            ..valid
        };
        assert!(before.preview_clipboard_payload(&caret).unwrap().is_none());
        document
            .apply(EditCommand::EditPreviewSelection {
                selection: caret,
                edit: crate::HtmlTextEdit::Replace(String::new()),
            })
            .unwrap();
        assert!(Arc::ptr_eq(&document.snapshot().0, &before.0));
        assert!(matches!(document.undo(), Err(DocumentError::NothingToUndo)));
    }

    #[test]
    fn preview_range_preserves_unselected_html_and_rejects_opaque_selected_content() {
        let source = "<div>Outside</div>\n\nBefore\n\n<div><custom-widget>Keep structure</custom-widget></div>\n\nAfter\n";
        let mut document = Document::from_markdown(source).unwrap();
        let before = document.snapshot();
        assert!(matches!(
            before.blocks().get(2).unwrap().as_ref(),
            BlockNode::PreservedSource { .. }
        ));
        let selection = PreviewSelection {
            revision: before.revision(),
            anchor: endpoint(&before, 1, 0),
            head: endpoint(&before, 3, 5),
        };
        assert!(
            before.preview_clipboard_payload(&selection).is_err(),
            "{:?}",
            before.blocks()
        );
        assert!(
            document
                .apply(EditCommand::EditPreviewSelection {
                    selection,
                    edit: crate::HtmlTextEdit::Replace("X".into())
                })
                .is_err()
        );
        assert!(Arc::ptr_eq(&document.snapshot().0, &before.0));
        assert!(matches!(document.undo(), Err(DocumentError::NothingToUndo)));

        let selection = PreviewSelection {
            revision: before.revision(),
            anchor: endpoint(&before, 1, 0),
            head: endpoint(&before, 1, 2),
        };
        document
            .apply(EditCommand::EditPreviewSelection {
                selection,
                edit: crate::HtmlTextEdit::Replace("X".into()),
            })
            .unwrap();
        for index in [0, 2, 3] {
            assert!(Arc::ptr_eq(
                before.blocks().get(index).unwrap(),
                document.snapshot().blocks().get(index).unwrap()
            ));
        }
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }

    #[test]
    fn reversed_single_paragraph_formatting_and_links_keep_selection_direction() {
        for edit in [
            crate::HtmlTextEdit::Format(crate::InlineFormat::Bold),
            crate::HtmlTextEdit::Link(Some("https://example.test".into())),
        ] {
            let mut document = Document::from_markdown("Alpha beta\n").unwrap();
            let before = document.snapshot();
            let node = before.blocks().get(0).unwrap().id();
            let selection = Selection::Text(TextSelection {
                anchor: DocumentPosition::new(node, 5, crate::Affinity::Downstream),
                head: DocumentPosition::new(node, 0, crate::Affinity::Upstream),
            });
            document
                .apply(EditCommand::SetSelection(selection.clone()))
                .unwrap();
            document.apply(edit.into_command()).unwrap();
            assert_eq!(document.snapshot().selection(), &selection);
            let snapshot = document.snapshot();
            let text = snapshot.node(node).unwrap().text().unwrap();
            assert!(
                text.runs()
                    .iter()
                    .any(|run| run.range == (0..5) && !run.styles.is_empty())
            );
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), "Alpha beta\n");
        }
    }

    #[test]
    fn preview_selection_rejects_intervening_edits_undo_and_active_composition() {
        let mut document = Document::from_markdown(SOURCE).unwrap();
        let before = document.snapshot();
        let selection = PreviewSelection {
            revision: before.revision(),
            anchor: endpoint(&before, 1, 2),
            head: endpoint(&before, 3, 4),
        };
        let node_id = before.blocks().get(0).unwrap().id();
        document
            .apply(EditCommand::ReplaceText {
                node_id,
                range: 0..0,
                text: "X".into(),
                selection_after: None,
                typing: false,
            })
            .unwrap();
        for undo_first in [false, true] {
            if undo_first {
                document.undo().unwrap();
            }
            let current = document.snapshot();
            assert!(current.preview_clipboard_payload(&selection).is_err());
            assert!(
                document
                    .apply(EditCommand::EditPreviewSelection {
                        selection: selection.clone(),
                        edit: crate::HtmlTextEdit::Replace("X".into())
                    })
                    .is_err()
            );
            assert!(Arc::ptr_eq(&current.0, &document.snapshot().0));
        }
        assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
        let selection = PreviewSelection {
            revision: document.snapshot().revision(),
            ..selection
        };
        document
            .begin_composition(TextSelection::caret(DocumentPosition::new(
                node_id,
                0,
                crate::Affinity::Downstream,
            )))
            .unwrap();
        assert!(matches!(
            document.apply(EditCommand::EditPreviewSelection {
                selection,
                edit: crate::HtmlTextEdit::Replace("X".into())
            }),
            Err(DocumentError::CompositionAlreadyActive)
        ));
        document.cancel_composition().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
    }

    #[test]
    fn html_only_composition_retires_only_the_imported_caret_host() {
        let source = "<div><p>First</p><p>Second</p></div>\n";
        for reverse in [false, true] {
            for commit in [false, true] {
                let mut document = Document::from_markdown(source).unwrap();
                let before = document.snapshot();
                let mut anchor = endpoint(&before, 0, 2);
                let mut head = endpoint(&before, 0, 3);
                let PreviewPosition::Html { position, .. } = &mut head else {
                    unreachable!()
                };
                position.text_node = 1;
                if reverse {
                    std::mem::swap(&mut anchor, &mut head);
                }
                document
                    .begin_preview_composition(&PreviewSelection {
                        revision: before.revision(),
                        anchor,
                        head,
                    })
                    .unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), source);
                for text in ["仮", "😀", "確定"] {
                    let after = document.update_composition(text.into()).unwrap();
                    assert_eq!(after.serialize().unwrap(), format!("Fi{text}ond\n"));
                    assert_eq!(after.blocks().len(), 1);
                }
                if commit {
                    document.commit_composition().unwrap();
                    document.undo().unwrap();
                } else {
                    document.cancel_composition().unwrap();
                }
                let restored = document.snapshot();
                assert_eq!(restored.serialize().unwrap(), source);
                assert_eq!(restored.selection(), before.selection());
                assert!(Arc::ptr_eq(
                    restored.blocks().get(1).unwrap(),
                    before.blocks().get(1).unwrap()
                ));
                assert!(matches!(document.undo(), Err(DocumentError::NothingToUndo)));
            }
        }
    }

    #[test]
    fn preview_range_can_end_in_the_transient_caret_host() {
        let source = "<p>First</p>\n";
        for reverse in [false, true] {
            let mut document = Document::from_markdown(source).unwrap();
            let before = document.snapshot();
            let mut anchor = endpoint(&before, 0, 2);
            let mut head = endpoint(&before, 1, 0);
            if reverse {
                std::mem::swap(&mut anchor, &mut head);
            }
            document
                .apply(EditCommand::EditPreviewSelection {
                    selection: PreviewSelection {
                        revision: before.revision(),
                        anchor,
                        head,
                    },
                    edit: crate::HtmlTextEdit::Replace("X".into()),
                })
                .unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), "FiX\n");
            assert_eq!(document.snapshot().blocks().len(), 1);
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        }
    }

    #[test]
    fn preview_endpoints_in_distinct_leaves_of_one_fragment_resolve_together() {
        let source = "<div><p>First</p><p>Second</p></div>\n";
        for reverse in [false, true] {
            let mut document = Document::from_markdown(source).unwrap();
            let before = document.snapshot();
            let anchor = endpoint(&before, 0, 2);
            let mut head = endpoint(&before, 0, 3);
            let PreviewPosition::Html { position, .. } = &mut head else {
                unreachable!()
            };
            position.text_node = 1;
            let mut selection = PreviewSelection {
                revision: before.revision(),
                anchor,
                head,
            };
            if reverse {
                std::mem::swap(&mut selection.anchor, &mut selection.head);
            }
            assert_eq!(
                before
                    .preview_clipboard_payload(&selection)
                    .unwrap()
                    .unwrap()
                    .plain_text
                    .as_deref(),
                Some("rst\nSec")
            );
            document
                .apply(EditCommand::EditPreviewSelection {
                    selection,
                    edit: crate::HtmlTextEdit::Replace("X".into()),
                })
                .unwrap();
            assert_eq!(
                document.snapshot().blocks().get(0).unwrap().plain_text(),
                "FiXond"
            );
            assert_eq!(document.snapshot().serialize().unwrap(), "FiXond\n");
            assert_eq!(document.snapshot().blocks().len(), 1);
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        }
    }
}
