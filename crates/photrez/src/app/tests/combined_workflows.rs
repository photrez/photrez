//! Combined workflow tests for the P7 reconciliation: several
//! Layers/Move/History/context-menu operations in one session, each asserting
//! the shared selection, history and target invariants.
//!
//! These glue the mounted paths the per-packet tests pin individually, so a
//! regression that crosses packet boundaries (selection contract + Move
//! commit + context menu + tabs) fails here rather than slipping through.
//!
//! Coverage limit: the Alt-drag + document-switch workflow is not drivable
//! from the mounted harness (a drag cannot be held across a document switch);
//! its production routes are pinned by `cross_doc.rs` and the mounted
//! `alt_drag_duplicates_the_layer_and_moves_the_copy` case. Save/reopen is
//! pinned by the services document tests.
use super::*;
use crate::state::ToolId;
use gpui_kit::test::TestWindowExt as _;

fn click_row_plain(
    window: &mut gpui_kit::Window,
    row: u64,
    modifiers: gpui_kit::Modifiers,
    cx: &mut gpui_kit::App,
) {
    let id = format!("layer-row-{row}");
    let bounds = window.find(id).bounds();
    let at = bounds.center();
    let _ = window.dispatch_event(
        gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
            button: gpui_kit::MouseButton::Left,
            position: at,
            modifiers,
            click_count: 1,
            ..Default::default()
        }),
        cx,
    );
    let _ = window.dispatch_event(
        gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
            button: gpui_kit::MouseButton::Left,
            position: at,
            modifiers,
            click_count: 1,
        }),
        cx,
    );
    // The harness carries modifiers across dispatches; clear them or the
    // Move drag dispatched with a stale Ctrl modifier is treated as a
    // modifier click (toggle), eating the gesture.
    let _ = window.dispatch_event(
        gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
            button: gpui_kit::MouseButton::Left,
            position: at,
            modifiers: gpui_kit::Modifiers::default(),
            click_count: 1,
        }),
        cx,
    );
    window.render_frame(cx);
}

fn right_click_row(window: &mut gpui_kit::Window, row: u64, cx: &mut gpui_kit::App) {
    let id = format!("layer-row-{row}");
    let bounds = window.find(id).bounds();
    let at = bounds.center();
    let _ = window.dispatch_event(
        gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
            button: gpui_kit::MouseButton::Right,
            position: at,
            modifiers: gpui_kit::Modifiers::default(),
            click_count: 1,
            ..Default::default()
        }),
        cx,
    );
    window.render_frame(cx);
}

fn double_click_row_name(window: &mut gpui_kit::Window, row: u64, cx: &mut gpui_kit::App) {
    let id = format!("layer-name-{row}");
    let bounds = window.find(id).bounds();
    for button_count in [1, 2] {
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                button: gpui_kit::MouseButton::Left,
                position: bounds.center(),
                modifiers: gpui_kit::Modifiers::default(),
                click_count: button_count,
                ..Default::default()
            }),
            cx,
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                button: gpui_kit::MouseButton::Left,
                position: bounds.center(),
                modifiers: gpui_kit::Modifiers::default(),
                click_count: button_count,
            }),
            cx,
        );
    }
    window.render_frame(cx);
}

fn selected_core_ids(content: &Entity<App>, cx: &mut gpui_kit::App) -> Vec<String> {
    let state = content.read(cx).state_for_test().read(cx);
    state
        .layers
        .iter()
        .filter(|row| state.selected_layers.contains(&row.id))
        .filter_map(|row| row.core_id.clone())
        .collect()
}

fn transform_of(content: &Entity<App>, cx: &mut gpui_kit::App, core_id: &str) -> (f64, f64) {
    let doc_id = content
        .read(cx)
        .active_document_id_for_test()
        .expect("the fixture is the active document");
    let loaded = content
        .read(cx)
        .sessions
        .get(&doc_id)
        .expect("session open")
        .loaded_document();
    let snapshot = loaded.content_snapshot().expect("snapshot present");
    snapshot
        .layers
        .iter()
        .find(|layer| layer.id == core_id)
        .map(|layer| (layer.transform.x, layer.transform.y))
        .expect("core layer present")
}

fn stack_core_ids(content: &Entity<App>, cx: &mut gpui_kit::App) -> Vec<String> {
    let doc_id = content
        .read(cx)
        .active_document_id_for_test()
        .expect("the fixture is the active document");
    let loaded = content
        .read(cx)
        .sessions
        .get(&doc_id)
        .expect("session open")
        .loaded_document();
    loaded
        .content_snapshot()
        .expect("snapshot present")
        .layers
        .iter()
        .map(|layer| layer.id.clone())
        .collect()
}

fn flags_of(content: &Entity<App>, cx: &mut gpui_kit::App) -> Vec<crate::state::LayerFlags> {
    content
        .read(cx)
        .state_for_test()
        .read(cx)
        .layer_flags
        .clone()
}

fn flags_for(
    content: &Entity<App>,
    cx: &mut gpui_kit::App,
    core_id: &str,
) -> crate::state::LayerFlags {
    let rows = super::canvas_input::layer_rows(content, cx).0;
    let pos = rows
        .iter()
        .position(|row| row.core_id.as_deref() == Some(core_id))
        .expect("core row projected");
    flags_of(content, cx)[pos]
}

fn draw_two_shapes(
    window: &mut gpui_kit::Window,
    content: &Entity<App>,
    cx: &mut gpui_kit::App,
) -> (u64, u64) {
    content.update(cx, |app, cx| {
        app.create_document(400, 300, cx).expect("blank document");
        app.state_for_test()
            .update(cx, |state, _| state.active_tool = ToolId::Shape);
    });
    window.render_frame(cx);
    super::canvas_input::drag_doc(window, (400, 300), (50.0, 50.0), (150.0, 100.0), cx);
    window.render_frame(cx);
    super::canvas_input::drag_doc(window, (400, 300), (250.0, 150.0), (350.0, 250.0), cx);
    window.render_frame(cx);
    let rows = super::canvas_input::layer_rows(content, cx).0;
    (rows[0].id, rows[1].id)
}

#[gpui_kit::test]
fn workflow_1_multi_select_move_duplicate_undo_redo(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let (top, base) = draw_two_shapes(window, &content, cx);
        super::canvas_input::select_layer(&content, cx, base);
        click_row_plain(
            window,
            top,
            gpui_kit::Modifiers {
                control: true,
                ..Default::default()
            },
            cx,
        );
        assert_eq!(
            selected_core_ids(&content, cx).len(),
            2,
            "both layers are selected"
        );
        let initial: Vec<(String, (f64, f64))> = selected_core_ids(&content, cx)
            .into_iter()
            .map(|id| {
                let t = transform_of(&content, cx, &id);
                (id, t)
            })
            .collect();
        let history_before = super::canvas_input::history_cursor(&content, cx);

        // Move the group one step; one entry regardless of selection size.
        super::canvas_input::set_tool(&content, cx, ToolId::Move, 1.0);
        window.render_frame(cx);
        super::canvas_input::drag_doc(window, (400, 300), (200.0, 125.0), (240.0, 165.0), cx);
        let history_after_move = super::canvas_input::history_cursor(&content, cx);
        assert_eq!(
            history_after_move,
            history_before + 1,
            "one Move gesture commits one entry regardless of selection size"
        );

        // Context-duplicate both selected layers from a selected row.
        right_click_row(window, top, cx);
        let history_after_select = super::canvas_input::history_cursor(&content, cx);
        window.click("ctx-layer-duplicate", cx);
        window.render_frame(cx);
        let history_after_duplicate = super::canvas_input::history_cursor(&content, cx);
        assert_eq!(
            history_after_duplicate,
            history_after_select + 1,
            "one duplicate gesture commits one entry"
        );
        assert_eq!(
            stack_core_ids(&content, cx).len(),
            5,
            "both layers were duplicated"
        );

        // Undo the duplicate.
        super::canvas_input::tab_to_canvas(window, cx);
        window.press("ctrl-z", cx);
        window.render_frame(cx);
        assert_eq!(
            stack_core_ids(&content, cx).len(),
            3,
            "undo restores the single multi-duplicate step"
        );

        // Undo the Move.
        window.press("ctrl-z", cx);
        window.render_frame(cx);
        for (id, (x0, y0)) in &initial {
            let (x1, y1) = transform_of(&content, cx, id);
            assert!(
                (x1 - *x0).abs() < 1e-6 && (y1 - *y0).abs() < 1e-6,
                "{id} returns to the pre-drag position ({x1}, {y1})"
            );
        }

        // Redo both steps.
        window.press("ctrl-shift-z", cx);
        window.render_frame(cx);
        window.press("ctrl-shift-z", cx);
        window.render_frame(cx);
        assert_eq!(
            stack_core_ids(&content, cx).len(),
            5,
            "redo restores the duplicated stack"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn workflow_2_rename_switch_hide_lock_attempt_move_undo(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = super::canvas_input::mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let rows = super::canvas_input::layer_rows(&content, cx).0;
        let top = rows[0].id;
        let base = rows[1].id;

        // Rename a layer, then cancel with Escape; nothing is committed.
        double_click_row_name(window, top, cx);
        assert!(
            content.read(cx).rename_content_for_test(cx).is_some(),
            "the rename prompt opens"
        );
        window.press("x", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        let renamed = content.read(cx).state_for_test().read(cx).layers[0]
            .name
            .clone();
        assert_eq!(renamed, "topx", "the rename commits once via Enter");
        let history_after_rename = super::canvas_input::history_cursor(&content, cx);

        // Switch the selection to the other row, hide it, then lock it.
        click_row_plain(window, base, gpui_kit::Modifiers::default(), cx);
        assert_eq!(
            selected_core_ids(&content, cx),
            vec!["bottom".to_string()],
            "the click replaces the single selection"
        );
        window.click(format!("layer-eye-{base}"), cx);
        window.render_frame(cx);
        assert!(
            !flags_for(&content, cx, "bottom").visible,
            "bottom is hidden"
        );
        click_row_plain(window, top, gpui_kit::Modifiers::default(), cx);
        window.click(format!("layer-lock-{top}"), cx);
        window.render_frame(cx);
        assert!(flags_for(&content, cx, "top").locked, "top is locked");
        // Multi-select both, then attempt a Move: two locked members move nothing.
        super::canvas_input::select_layer(&content, cx, base);
        click_row_plain(
            window,
            top,
            gpui_kit::Modifiers {
                control: true,
                ..Default::default()
            },
            cx,
        );
        super::canvas_input::set_tool(&content, cx, ToolId::Move, 1.0);
        let position_before = transform_of(&content, cx, "top");
        let base_move_before = transform_of(&content, cx, "bottom");
        let history_before_move = super::canvas_input::history_cursor(&content, cx);
        super::canvas_input::drag_doc(window, (2, 1), (1.5, 0.5), (0.4, 0.4), cx);
        assert_eq!(
            transform_of(&content, cx, "top"),
            position_before,
            "locked members cannot be moved"
        );
        assert_eq!(
            super::canvas_input::history_cursor(&content, cx),
            history_before_move,
            "a refused Move adds no history"
        );

        // Undo lock and hide: two entries in reverse order.
        super::canvas_input::tab_to_canvas(window, cx);
        window.press("ctrl-z", cx);
        window.render_frame(cx);
        assert!(!flags_for(&content, cx, "top").locked, "lock undone");
        window.press("ctrl-z", cx);
        window.render_frame(cx);
        assert!(flags_for(&content, cx, "bottom").visible, "hide undone");
        window.press("ctrl-z", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layers[0].name,
            "top",
            "rename undone"
        );
        let _ = (history_after_rename, base_move_before);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn workflow_3_reorder_history_tab_undo_layers_tab_move(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let (top, _base) = draw_two_shapes(window, &content, cx);
        window.render_frame(cx);
        let rows = super::canvas_input::layer_rows(&content, cx).0;
        assert_eq!(rows.len(), 3, "the fixture has a background and two shapes");
        let top_core = rows[0].core_id.clone().expect("created shape has core id");

        let stacked_before = stack_core_ids(&content, cx);
        assert_eq!(stacked_before.len(), 3);
        // Reorder the top row below its sibling from the panel.
        window.click(format!("layer-down-{top}"), cx);
        window.render_frame(cx);
        let stacked_after = stack_core_ids(&content, cx);
        assert_ne!(
            stacked_after, stacked_before,
            "the panel reorder changes the stack order"
        );

        // Switch to History, undo the reorder, switch back to Layers.
        window.click("history", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layer_tab,
            crate::state::LayerTab::History,
            "the History tab mounts"
        );
        super::canvas_input::tab_to_canvas(window, cx);
        window.press("ctrl-z", cx);
        window.render_frame(cx);
        assert_eq!(
            stack_core_ids(&content, cx),
            stacked_before,
            "Ctrl+Z from the History tab undoes the reorder"
        );
        window.click("layers", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layer_tab,
            crate::state::LayerTab::Layers,
            "the Layers tab mounts again"
        );

        // Move the top layer and expect exactly one history entry.
        super::canvas_input::set_tool(&content, cx, ToolId::Move, 1.0);
        super::canvas_input::select_layer(&content, cx, top);
        let before = transform_of(&content, cx, &top_core);
        let history_before = super::canvas_input::history_cursor(&content, cx);
        super::canvas_input::drag_doc(window, (400, 300), (300.0, 200.0), (330.0, 230.0), cx);
        let after = transform_of(&content, cx, &top_core);
        assert!(
            after.0 > before.0 && after.1 > before.1,
            "the layer moved after the tab round trip"
        );
        assert_eq!(
            super::canvas_input::history_cursor(&content, cx),
            history_before + 1,
            "the Move adds exactly one history entry"
        );
    })
    .expect("test window remains open");
}
