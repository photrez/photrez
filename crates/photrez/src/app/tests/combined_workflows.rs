//! Combined workflow tests for the P7 reconciliation: several
//! Layers/Move/History/context-menu operations in one session, each asserting
//! the shared selection, history and target invariants.
//!
//! These glue the mounted paths the per-packet tests pin individually, so a
//! regression that crosses packet boundaries (selection contract + Move
//! commit + context menu + tabs) fails here rather than slipping through.
//!
//! Workflow 4 (Alt-drag → document switch → delayed release) drives the
//! production handler the mounted row drop invokes
//! (`App::drop_layer_on_row` with `Alt` read at release), exercising both
//! documents and both histories. The mounted harness cannot hold a live drag
//! across the switch itself; the drag payload is created at drag start and
//! the release event is dispatched after switching documents, which is the
//! exact semantic the production row handler implements.
//!
//! Save/reopen is pinned by the services document tests.
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

/// Tab hover-to-switch: during a layer drag, hovering a document tab for
/// 500 ms activates that document, leaving the tab cancels the timer, the
/// active tab arms nothing, and switching requires an active drag.
#[gpui_kit::test]
fn tab_hover_switches_documents_during_a_layer_drag(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = super::canvas_input::mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    let (doc_a, doc_b) = cx
        .update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            let doc_a = content
                .read(cx)
                .active_document_id_for_test()
                .expect("the archive is the active document");
            content
                .update(cx, |app, cx| app.create_document(64, 64, cx))
                .expect("the second document is created");
            let doc_b = content
                .read(cx)
                .active_document_id_for_test()
                .expect("the new document is active");
            assert_ne!(doc_a, doc_b);

            activate_by_id(&content, cx, &doc_a);
            window.render_frame(cx);

            let state = content.read(cx).state_for_test().clone();
            state.update(cx, |s, _| s.layer_drag_active = true);
            window.render_frame(cx);

            window.hover(format!("document-tab-{doc_b}"), cx);
            window.render_frame(cx);
            assert_eq!(
                content.read(cx).active_document_id_for_test().as_deref(),
                Some(doc_a.as_str()),
                "hovering before the delay has elapsed keeps the current document"
            );
            (doc_a, doc_b)
        })
        .expect("test window remains open");

    cx.background_executor
        .advance_clock(std::time::Duration::from_millis(650));
    cx.run_until_parked();
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_document_id_for_test().as_deref(),
            Some(doc_b.as_str()),
            "hovering a tab for 500 ms mid-drag switches to its document"
        );
        window.hover(format!("document-tab-{doc_a}"), cx);
        window.render_frame(cx);
        window.hover(format!("document-tab-{doc_b}"), cx);
        window.render_frame(cx);
    })
    .expect("test window remains open");

    cx.background_executor
        .advance_clock(std::time::Duration::from_millis(650));
    cx.run_until_parked();
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_document_id_for_test().as_deref(),
            Some(doc_b.as_str()),
            "leaving an armed tab cancels its hover timer"
        );
        let state = content.read(cx).state_for_test().clone();
        state.update(cx, |s, _| s.layer_drag_active = false);
        window.hover(format!("document-tab-{doc_a}"), cx);
        window.render_frame(cx);
    })
    .expect("test window remains open");

    cx.background_executor
        .advance_clock(std::time::Duration::from_millis(650));
    cx.run_until_parked();
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_document_id_for_test().as_deref(),
            Some(doc_b.as_str()),
            "without an active drag, hovering a tab never switches documents"
        );
    })
    .expect("test window remains open");
}

/// Press and move a row, but hold the release so the mid-drag flag can be
/// observed before the row's mouse-up cleanup runs.
fn drag_row_without_release(
    window: &mut gpui_kit::Window,
    from: gpui_kit::Point<gpui_kit::Pixels>,
    to: gpui_kit::Point<gpui_kit::Pixels>,
    cx: &mut gpui_kit::App,
) {
    window.render_frame(cx);
    let _ = window.dispatch_event(
        gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
            position: from,
            pressed_button: None,
            modifiers: gpui_kit::Modifiers::default(),
        }),
        cx,
    );
    let _ = window.dispatch_event(
        gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
            button: gpui_kit::MouseButton::Left,
            position: from,
            click_count: 1,
            ..Default::default()
        }),
        cx,
    );
    for step in 1..=8 {
        let fraction = step as f32 / 8.0;
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: gpui_kit::point(
                    from.x + (to.x - from.x) * fraction,
                    from.y + (to.y - from.y) * fraction,
                ),
                pressed_button: Some(gpui_kit::MouseButton::Left),
                modifiers: gpui_kit::Modifiers::default(),
            }),
            cx,
        );
    }
    window.render_frame(cx);
}

/// The mounted drag path reaches the row's `on_drag`: an unlocked row arms
/// the cross-document drag flag. This is the capability check for the locked
/// row test below.
#[gpui_kit::test]
fn unlocked_layer_row_drag_arms_the_drag_flag(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = super::canvas_input::mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        let state = content.read(cx).state_for_test().clone();
        state.update(cx, |s, _| s.layer_tab = crate::state::LayerTab::Layers);
        window.render_frame(cx);
        let from = window.find("layer-row-0").bounds().center();
        let to = window.find("layer-row-1").bounds().center();
        drag_row_without_release(window, from, to, cx);
        assert!(
            state.read(cx).layer_drag_active,
            "the harness must reach the row's drag-start handler"
        );
        assert_eq!(
            state.read(cx).layer_drag_source,
            state
                .read(cx)
                .layers
                .first()
                .and_then(|row| row.core_id.clone()),
            "the source row is tracked"
        );
        assert_eq!(
            window.find("layer-row-0").label(),
            Some("Dragging layer"),
            "the source row takes the dimmed, announced drag state"
        );
    })
    .expect("test window remains open");
}

/// The reference refuses to start a layer drag on a locked row (a background
/// layer is the exception). No drag means no drag flag and no reorder, even
/// when the pointer moves onto another row.
#[gpui_kit::test]
fn locked_layer_row_does_not_start_a_drag(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = super::canvas_input::mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        let state = content.read(cx).state_for_test().clone();
        state.update(cx, |s, _| s.layer_tab = crate::state::LayerTab::Layers);
        window.render_frame(cx);
        let core_id = state
            .read(cx)
            .layers
            .first()
            .and_then(|row| row.core_id.clone())
            .expect("the top row has a core id");
        content.update(cx, |app, cx| app.toggle_layer_locked(core_id, cx));
        window.render_frame(cx);

        let from = window.find("layer-row-0").bounds().center();
        let to = window.find("layer-row-1").bounds().center();
        drag_row_without_release(window, from, to, cx);
        assert!(
            !state.read(cx).layer_drag_active,
            "a locked row must not arm a cross-document drag"
        );
        let ids: Vec<u64> = state.read(cx).layers.iter().map(|row| row.id).collect();
        assert_eq!(ids, vec![0, 1], "no drag means no reorder");
    })
    .expect("test window remains open");
}

fn doc_stack_ids(content: &Entity<App>, cx: &gpui_kit::App, doc_id: &str) -> Vec<String> {
    let loaded = content
        .read(cx)
        .sessions
        .get(doc_id)
        .expect("document is open")
        .loaded_document();
    loaded
        .content_snapshot()
        .expect("document content is present")
        .layers
        .iter()
        .map(|layer| layer.id.clone())
        .collect()
}

fn activate_by_id(content: &Entity<App>, cx: &mut gpui_kit::App, doc_id: &str) {
    content.update(cx, |app, cx| {
        app.activate_document(doc_id, cx).expect("document opens");
    });
}

#[gpui_kit::test]
fn workflow_4_alt_move_after_document_switch_moves_layer_and_advances_both_histories(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = super::canvas_input::mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let doc_a = content
            .read(cx)
            .active_document_id_for_test()
            .expect("archive is the active document");
        content
            .update(cx, |app, cx| app.create_document(64, 64, cx))
            .expect("second document is created");
        let doc_b = content
            .read(cx)
            .active_document_id_for_test()
            .expect("the blank document is active");
        assert_ne!(doc_a, doc_b, "the two documents are distinct");
        let doc_b_cursor_before = super::canvas_input::history_cursor(&content, cx);
        let doc_b_layers_before = doc_stack_ids(&content, cx, &doc_b);

        // Back to docA; this is the document the drag started in.
        activate_by_id(&content, cx, &doc_a);
        let rows = super::canvas_input::layer_rows(&content, cx).0;
        assert_eq!(rows.len(), 2, "the archive has two layers");
        let source_row = rows[0].clone();
        let source_core = source_row
            .core_id
            .clone()
            .expect("archive layer has core id");
        let source_path = content
            .read(cx)
            .core_layer_pixels_for_test(&source_core)
            .expect("source pixels are readable");
        let payload = crate::app::cross_doc::LayerDragPayload {
            source_doc_id: doc_a.clone(),
            core_id: Some(source_core.clone()),
            row_id: source_row.id,
        };
        let doc_a_cursor_before = super::canvas_input::history_cursor(&content, cx);

        // Mid-drag: switch documents instead of holding the drag; the release
        // lands in docB through the same production path the mounted row drop
        // drives.
        activate_by_id(&content, cx, &doc_b);
        let target_row = super::canvas_input::layer_rows(&content, cx).0[0].id;
        content.update(cx, |app, cx| {
            app.drop_layer_on_row(
                &payload,
                target_row,
                crate::app::cross_doc::InsertPosition::Above,
                true,
                cx,
            );
        });
        window.render_frame(cx);

        // Doc B: the moved layer is present as the new topmost entry and its
        // pixels match the source.
        let doc_b_stack = doc_stack_ids(&content, cx, &doc_b);
        assert_eq!(
            doc_b_stack.len(),
            doc_b_layers_before.len() + 1,
            "docB gained the moved layer"
        );
        let moved_layer = doc_b_stack
            .iter()
            .find(|id| !doc_b_layers_before.contains(id))
            .expect("the new layer is in docB's stack")
            .clone();
        let moved_pixels = content
            .read(cx)
            .core_layer_pixels_for_test(&moved_layer)
            .expect("moved layer pixels are readable");
        assert_eq!(
            moved_pixels.2, source_path.2,
            "docB holds the source pixels byte for byte"
        );

        // Doc A: the moved layer is gone; bottom remains.
        let doc_a_stack = doc_stack_ids(&content, cx, &doc_a);
        assert_eq!(
            doc_a_stack,
            vec!["bottom".to_string()],
            "docA lost the moved layer"
        );

        // Histories: both documents got exactly one new entry each.
        let doc_b_cursor_after = super::canvas_input::history_cursor(&content, cx);
        assert_eq!(
            doc_b_cursor_after,
            doc_b_cursor_before + 1,
            "docB recorded the AddLayerWithState entry exactly once"
        );
        activate_by_id(&content, cx, &doc_a);
        assert_eq!(
            super::canvas_input::history_cursor(&content, cx),
            doc_a_cursor_before + 1,
            "docA recorded the DeleteLayer entry exactly once"
        );
        activate_by_id(&content, cx, &doc_b);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn workflow_4_plain_drag_after_document_switch_copies_and_keeps_the_source_selection(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = super::canvas_input::mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let doc_a = content
            .read(cx)
            .active_document_id_for_test()
            .expect("archive is the active document");
        content
            .update(cx, |app, cx| app.create_document(64, 64, cx))
            .expect("second document is created");
        let doc_b = content
            .read(cx)
            .active_document_id_for_test()
            .expect("the blank document is active");
        let doc_b_cursor_before = super::canvas_input::history_cursor(&content, cx);
        let doc_b_layers_before = doc_stack_ids(&content, cx, &doc_b);

        activate_by_id(&content, cx, &doc_a);
        let rows = super::canvas_input::layer_rows(&content, cx).0;
        let top_row = rows[0].clone();
        let source_core = top_row.core_id.clone().expect("archive layer has core id");
        let payload = crate::app::cross_doc::LayerDragPayload {
            source_doc_id: doc_a.clone(),
            core_id: Some(source_core.clone()),
            row_id: top_row.id,
        };
        let doc_a_cursor_before = super::canvas_input::history_cursor(&content, cx);

        activate_by_id(&content, cx, &doc_b);
        let target_row = super::canvas_input::layer_rows(&content, cx).0[0].id;
        content.update(cx, |app, cx| {
            app.drop_layer_on_row(
                &payload,
                target_row,
                crate::app::cross_doc::InsertPosition::Above,
                false,
                cx,
            );
        });
        window.render_frame(cx);

        let doc_b_stack = doc_stack_ids(&content, cx, &doc_b);
        assert_eq!(
            doc_b_stack.len(),
            doc_b_layers_before.len() + 1,
            "docB gained the copied layer"
        );
        let doc_a_stack = doc_stack_ids(&content, cx, &doc_a);
        assert_eq!(
            doc_a_stack,
            vec!["top".to_string(), "bottom".to_string()],
            "docA keeps both source layers on a plain drag"
        );
        activate_by_id(&content, cx, &doc_a);
        assert_eq!(
            super::canvas_input::history_cursor(&content, cx),
            doc_a_cursor_before,
            "docA records no deletion on a plain copy"
        );
        activate_by_id(&content, cx, &doc_b);
        assert_eq!(
            super::canvas_input::history_cursor(&content, cx),
            doc_b_cursor_before + 1,
            "docB recorded the AddLayerWithState entry once"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn workflow_4_alt_move_refuses_the_last_source_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = super::canvas_input::mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let doc_a = content
            .read(cx)
            .active_document_id_for_test()
            .expect("archive is the active document");
        content
            .update(cx, |app, cx| app.create_document(64, 64, cx))
            .expect("second document is created");
        let doc_b = content
            .read(cx)
            .active_document_id_for_test()
            .expect("the blank document is active");
        let doc_b_layers_before = doc_stack_ids(&content, cx, &doc_b);
        let doc_b_fixture_note = doc_b_layers_before;

        activate_by_id(&content, cx, &doc_a);
        // First Alt move leaves docA with a single layer.
        let rows = super::canvas_input::layer_rows(&content, cx).0;
        let top_row = rows[0].clone();
        let rows_payload = crate::app::cross_doc::LayerDragPayload {
            source_doc_id: doc_a.clone(),
            core_id: top_row.core_id.clone(),
            row_id: top_row.id,
        };
        activate_by_id(&content, cx, &doc_b);
        let target_row = super::canvas_input::layer_rows(&content, cx).0[0].id;
        content.update(cx, |app, cx| {
            app.drop_layer_on_row(
                &rows_payload,
                target_row,
                crate::app::cross_doc::InsertPosition::Above,
                true,
                cx,
            );
        });
        window.render_frame(cx);
        assert_eq!(
            doc_stack_ids(&content, cx, &doc_a),
            vec!["bottom".to_string()],
            "docA is down to one layer after the first Alt move"
        );
        let doc_b_layers_before_refusal = doc_stack_ids(&content, cx, &doc_b);
        assert_eq!(
            doc_b_layers_before_refusal.len(),
            doc_b_fixture_note.len() + 1,
            "the first Alt move added exactly one layer to docB"
        );

        // A second Alt move out of docA's last layer must refuse and change
        // nothing on either side.
        let doc_a_cursor_before = {
            activate_by_id(&content, cx, &doc_a);
            super::canvas_input::history_cursor(&content, cx)
        };
        let last_row = super::canvas_input::layer_rows(&content, cx).0[0].clone();
        let sidebar = crate::app::cross_doc::LayerDragPayload {
            source_doc_id: doc_a.clone(),
            core_id: last_row.core_id.clone(),
            row_id: last_row.id,
        };
        activate_by_id(&content, cx, &doc_b);
        let doc_b_cursor_before = super::canvas_input::history_cursor(&content, cx);
        let target_row = super::canvas_input::layer_rows(&content, cx).0[0].id;
        content.update(cx, |app, cx| {
            app.drop_layer_on_row(
                &sidebar,
                target_row,
                crate::app::cross_doc::InsertPosition::Above,
                true,
                cx,
            );
        });
        window.render_frame(cx);

        assert_eq!(
            doc_stack_ids(&content, cx, &doc_a),
            vec!["bottom".to_string()],
            "the last layer stays in docA on the refused Alt move"
        );
        assert_eq!(
            doc_stack_ids(&content, cx, &doc_b),
            doc_b_layers_before_refusal,
            "docB is unchanged after the refusal"
        );
        activate_by_id(&content, cx, &doc_a);
        assert_eq!(
            super::canvas_input::history_cursor(&content, cx),
            doc_a_cursor_before,
            "the refusal records no deletion in docA"
        );
        activate_by_id(&content, cx, &doc_b);
        assert_eq!(
            super::canvas_input::history_cursor(&content, cx),
            doc_b_cursor_before,
            "the refusal records no addition in docB"
        );
    })
    .expect("test window remains open");
}
