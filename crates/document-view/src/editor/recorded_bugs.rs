//! Regressions for the four explicitly accepted September 8 Crusty bugs.
use super::*;
use crate::init_editor;

const CARDS: &str = "## Architecture\n\nUse three crates:\n\n- **Document core:** Content, selections, transactions and undo.\n- **Document view:** Shaping, arrangements, hit testing and tables.\n- **Application:** Windows, commands, files and recovery.\n";
const TABLES: &str = "## Reference\n\n### Typography\n\n| Role | Size |\n| --- | --- |\n| Body | 16 |\n| Title | 52 |\n\n### Palette\n\n| Role | Value |\n| --- | --- |\n| Paper | Warm |\n| Ink | Dark |\n";

#[gpui::test]
fn table_controls_have_only_three_pixel_knobs(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        init_editor(cx);
    });
    let (editor, cx) = cx.add_window_view(|window, cx| {
        RichDocumentEditor::new(Document::from_markdown(TABLES).unwrap(), window, cx)
    });
    let cx: &mut gpui::VisualTestContext = cx;
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
        editor.update(cx, |editor, cx| {
            editor.table_hover = editor
                .painted_lines
                .iter()
                .find_map(|line| editor.table_cell_geometry(line).map(|(target, _)| target));
            cx.notify();
        });
    });
    for zoom in [1., 1.5, 2.] {
        editor.update(cx, |editor, cx| {
            editor.zoom_factor = zoom;
            cx.notify();
        });
        cx.update(|window, cx| _ = window.draw(cx));
        for (edge, mark) in [
            ("table-edge-Top", "table-knob-Top"),
            ("table-edge-Right", "table-knob-Right"),
            ("table-edge-Bottom", "table-knob-Bottom"),
            ("table-edge-Left", "table-knob-Left"),
        ] {
            let hit = cx.debug_bounds(edge).unwrap();
            let knob = cx
                .debug_bounds(mark)
                .expect("real knob, not a hidden text label");
            let side = px((3_f32 * zoom).round());
            assert_eq!(knob.size, size(side, side));
            // GPUI rounds layout origins; an odd-sized knob can land half a
            // pixel away from the exact center of an even-sized hit target.
            assert!(f32::from(knob.center().x - hit.center().x).abs() <= 0.5);
            assert!(f32::from(knob.center().y - hit.center().y).abs() <= 0.5);
            if zoom == 1. {
                let target = editor.read_with(cx, |editor, _| editor.table_hover);
                cx.simulate_event(MouseMoveEvent {
                    position: hit.center(),
                    ..Default::default()
                });
                cx.run_until_parked();
                assert_eq!(
                    editor.read_with(cx, |editor, _| editor.table_hover),
                    target,
                    "moving onto an edge control must retain its target"
                );
            }
        }
    }
}

#[test]
fn measured_plan_md_entities_fit_a_bounded_card_row() {
    use crate::adaptive::candidates::{ItemMeasurement, choose_list};
    // Loaded-font measurements from the native 1600 px plan.md specimen:
    // one authored title plus up to four body lines, 24 px all-side insets.
    let decision = choose_list(3, 1280., 548., None, true, false, |item, _, cards| {
        Some(ItemMeasurement {
            lines: if cards {
                [5, 4, 3][item]
            } else {
                [3, 3, 2][item]
            },
            height: if cards {
                [168., 144., 120.][item]
            } else {
                [72., 72., 48.][item]
            },
            preferred_width: [1040.832, 915.552, 685.44][item],
            overflow: false,
        })
    });
    assert_eq!(
        decision.layout,
        ListLayout::Grid(3),
        "{:?}",
        decision.candidates
    );
}

#[gpui::test]
fn labelled_features_have_open_edges_and_no_marker_indent(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let document = Document::from_markdown(CARDS).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let fonts =
            FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
        let plan = build_measured_adaptive_plan(&projection, 1280., 1200., None, false, &fonts);
        let nodes = projection
            .segments()
            .iter()
            .filter(|s| s.context.list_depth == 1)
            .collect::<Vec<_>>();
        assert_eq!(nodes.len(), 3);
        let lines =
            build_measured_visual_lines(&projection, &HashMap::new(), 1280., &plan, Some(&fonts));
        for segment in nodes {
            let slot = plan
                .slots
                .get(&segment.node_id)
                .expect("three short independent objects fit");
            assert!(slot.cards);
            assert_eq!(
                slot.card_accent,
                crate::adaptive::CardAccent::OpenLabeled,
                "a bold label alone does not justify an enclosure"
            );
            let line = lines
                .iter()
                .find(|l| l.projected_start() == segment.projection_start())
                .unwrap();
            assert_eq!(
                line.inset, 0.,
                "open labels align to their track without a bullet or card inset"
            );
        }
        assert_eq!(document.snapshot().serialize().unwrap(), CARDS);
    });
}

#[gpui::test]
fn short_table_leadins_do_not_become_stranded_columns(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let source = "## Typography and palette\n\nThe reference defines shared typography for every document component.\n\nUse these text roles:\n\n| Role | Face | Size |\n| --- | --- | --- |\n| Display | Serif | 52 |\n| Section | Serif | 30 |\n| Subsection | Serif | 24 |\n| Body | Sans | 16 |\n| Caption | Sans | 13 |\n| Code | Mono | 14 |\n\nBundle fonts locally so documents remain readable offline.\n";
        let document = Document::from_markdown(source).unwrap();
        let mut projection = TextProjection::from_snapshot(&document.snapshot());
        let fonts = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
        fonts.measure_tables(&mut projection);
        for width in [1280., 900., 480.] {
            let plan = build_measured_adaptive_plan(&projection, width, 1600., None, false, &fonts);
            let lead = projection.segments().iter().find(|s| &projection.text()[s.projection_range()] == "Use these text roles:").unwrap();
            assert!(!plan.slots.contains_key(&lead.node_id), "a one-line table lead-in is not a parallel explanation at {width}px");
            let lines = build_measured_visual_lines(&projection, &HashMap::new(), width, &plan, Some(&fonts));
            let components = component_geometry(&projection, &lines, width, 1., &visual_line_paint_order(&lines));
            let table = projection.roots().find(|b| matches!(b, BlockNode::Table(_))).unwrap();
            assert_eq!(components.get(&table.id()).unwrap().left_fraction, 0.);
        }
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    });
}

#[gpui::test]
fn inserted_cells_inherit_the_current_table_placement_immediately(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let fonts = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
        for column in [false, true] {
            for index in 0..=if column { 2 } else { 3 } {
                let mut document = Document::from_markdown(TABLES).unwrap();
                let mut projection = TextProjection::from_snapshot(&document.snapshot());
                fonts.measure_tables(&mut projection);
                let initial = build_measured_adaptive_plan(&projection, 1280., 1200., None, false, &fonts);
                let tables = projection.roots().filter_map(|b| if let BlockNode::Table(t) = b { Some(t.id) } else { None }).collect::<Vec<_>>();
                let table = tables[1];
                let first = projection.segments().iter().find(|s| s.context.table_cell.is_some_and(|(id, _, _)| id == table)).unwrap().node_id;
                let placement = initial.slots[&first];
                assert!(placement.left(1280.) > 0., "the test must exercise a displaced table");
                document.apply(if column { EditCommand::InsertTableColumn { table_id: table, index } } else { EditCommand::InsertTableRow { table_id: table, index } }).unwrap();
                let mut edited = TextProjection::from_snapshot(&document.snapshot());
                fonts.measure_tables(&mut edited);
                let kept = arrangement::build_edit_locked_adaptive_plan(&edited, 1280., 1200., Some(&initial), true, &fonts, Some(first));
                for segment in edited.segments().iter().filter(|s| s.context.table_cell.is_some_and(|(id, _, _)| id == table)) {
                    assert_eq!(kept.slots.get(&segment.node_id), Some(&placement), "column={column} index={index}: every newly created cell must share the retained table slot");
                }
                // Synchronous geometry, before a worker can repair it: every
                // painted cell must use the same retained table placement.
                let lines = build_measured_visual_lines(&edited, &HashMap::new(), 1280., &kept, Some(&fonts));
                for line in lines.iter().filter(|line| line.table_cell.is_some_and(|(id, _, _, _)| id == table)) {
                    assert_eq!(line.slot, Some(placement));
                }
                let inserted = document.snapshot().serialize().unwrap();
                document.undo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), TABLES);
                document.redo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), inserted);
            }
        }
    });
}
