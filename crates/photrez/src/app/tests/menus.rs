use super::input::{drag_doc, layer_rows, page, select_layer, set_tool, window_point};
use super::*;
use crate::state::ToolId;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{MouseButton, MouseDownEvent, MouseUpEvent, PlatformInput};

fn open_canvas_menu(
    window: &mut gpui_kit::Window,
    doc: (u32, u32),
    at: (f32, f32),
    cx: &mut gpui_kit::App,
) {
    let (bounds, placement) = page(window, doc);
    let position = window_point(bounds, placement, doc, at.0, at.1);
    let _ = window.dispatch_event(
        PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Right,
            position,
            ..Default::default()
        }),
        cx,
    );
    window.render_frame(cx);
}

#[gpui_kit::test]
fn canvas_menu_disables_rows_without_a_selection_or_clipboard(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(200, 150, cx).expect("blank document");
        });
        window.render_frame(cx);
        open_canvas_menu(window, (200, 150), (100.0, 75.0), cx);
        assert!(
            window.try_find("ctx-paste").is_some(),
            "the right-click menu mounts"
        );
        let layers_before = content.read(cx).state_for_test().read(cx).layers.len();
        window.click("ctx-paste", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layers.len(),
            layers_before,
            "a disabled Paste inserts nothing"
        );
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .status_error
                .is_none(),
            "a disabled Paste must not run the command and surface its error"
        );
        window.click("ctx-copy", cx);
        assert!(
            content.read(cx).clipboard_for_test().is_none(),
            "a disabled Copy must not fill the clipboard"
        );
        window.click("ctx-deselect", cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .status_error
                .is_none(),
            "a disabled Deselect must not run with no selection"
        );
        assert!(
            window.try_find("ctx-paste").is_some(),
            "disabled rows leave the menu open"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn canvas_menu_enables_paste_once_the_clipboard_holds_a_selection(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(200, 150, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        drag_doc(window, (200, 150), (10.0, 10.0), (60.0, 60.0), cx);
        content.update(cx, |app, cx| {
            app.copy_selection(cx).expect("copy the pixel selection");
        });
        assert!(
            content.read(cx).clipboard_for_test().is_some(),
            "the fixture clipboard holds the copied pixels"
        );
        open_canvas_menu(window, (200, 150), (150.0, 120.0), cx);
        let layers_before = content.read(cx).state_for_test().read(cx).layers.len();
        window.click("ctx-paste", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layers.len(),
            layers_before + 1,
            "an enabled Paste inserts the clipboard as a layer"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn edit_menu_disables_rows_until_their_state_exists(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(200, 150, cx).expect("blank document");
        });
        window.render_frame(cx);
        window.click("menus.edit", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("menu-edit-undo").is_some(),
            "the Edit menu opens"
        );
        window.click("menu-edit-paste", cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .status_error
                .is_none(),
            "a disabled Paste must not run the command"
        );
        assert!(
            window.try_find("menu-edit-paste").is_some(),
            "disabled rows keep the menu open"
        );
        content.update(cx, |app, cx| {
            app.state_for_test()
                .update(cx, |state, _| state.open_menu = None);
            app.nudge_selected_layer(10.0, 0.0, cx).expect("nudge");
        });
        window.render_frame(cx);
        let nudged = content
            .read(cx)
            .selected_transform_for_test(cx)
            .expect("selected layer");
        window.click("menus.edit", cx);
        window.render_frame(cx);
        window.click("menu-edit-undo", cx);
        window.render_frame(cx);
        let undone = content
            .read(cx)
            .selected_transform_for_test(cx)
            .expect("selected layer");
        assert!(
            (undone.0 - (nudged.0 - 10.0)).abs() < 1e-6,
            "Undo is enabled once the history has an entry: {nudged:?} -> {undone:?}"
        );
    })
    .expect("test window remains open");
}

/// Cut/Copy are gated on the active layer carrying a pixel bitmap: a pixel
/// selection alone is not enough when the active layer has no pixels of its
/// own (a shape layer renders from parameters). Selecting the raster layer
/// with the same selection makes Copy live again.
#[gpui_kit::test]
fn canvas_menu_gates_cut_and_copy_on_the_active_layer_bitmap(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(200, 150, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);
        drag_doc(window, (200, 150), (100.0, 10.0), (150.0, 50.0), cx);
        let (rows, selected) = super::input::layer_rows(&content, cx);
        assert_eq!(rows.len(), 2, "the shape adds one layer to the Background");
        let shape = rows
            .iter()
            .find(|row| row.name == "Shape")
            .expect("the shape row exists");
        assert_eq!(selected, shape.id, "a new shape becomes the active layer");

        set_tool(&content, cx, ToolId::Selection, 1.0);
        drag_doc(window, (200, 150), (10.0, 10.0), (60.0, 60.0), cx);
        assert!(
            content.read(cx).pixel_selection_present(),
            "the selection tool commits a pixel selection"
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).selected_layer,
            shape.id,
            "the shape layer stays active while selecting pixels"
        );

        window.render_frame(cx);
        open_canvas_menu(window, (200, 150), (100.0, 75.0), cx);
        window.click("ctx-cut", cx);
        assert!(
            window.try_find("ctx-copy").is_some(),
            "a disabled Cut must leave the menu open"
        );
        window.click("ctx-copy", cx);
        assert!(
            content.read(cx).clipboard_for_test().is_none(),
            "Cut/Copy stay inert when the active layer has no bitmap"
        );
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .status_error
                .is_none(),
            "a disabled Cut/Copy must not surface a command error"
        );
        assert!(
            window.try_find("ctx-copy").is_some(),
            "disabled rows leave the menu open"
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layers.len(),
            2,
            "a disabled Cut must not remove the shape layer"
        );

        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.context_menu = None;
                state.open_menu = None;
            });
        });
        window.render_frame(cx);
        window.click("menus.edit", cx);
        window.render_frame(cx);
        window.click("menu-edit-copy", cx);
        assert!(
            content.read(cx).clipboard_for_test().is_none(),
            "the Edit menu applies the same bitmap gate as the context menu"
        );
        assert!(
            window.try_find("menu-edit-copy").is_some(),
            "a disabled Edit-menu Copy keeps the menu open"
        );
        content.update(cx, |app, cx| {
            app.state_for_test()
                .update(cx, |state, _| state.open_menu = None);
        });
        let background = rows
            .iter()
            .find(|row| row.name == "Background")
            .expect("the fixture Background row exists")
            .id;
        select_layer(&content, cx, background);
        window.render_frame(cx);
        open_canvas_menu(window, (200, 150), (100.0, 75.0), cx);
        window.click("ctx-copy", cx);
        assert!(
            content.read(cx).clipboard_for_test().is_some(),
            "Copy is enabled once the active layer owns pixels again"
        );
    })
    .expect("test window remains open");
}

/// Keyboard chords dispatch through the same enabled commands as
/// the menus; the native shortcut path used to bypass the disabled rows.
#[gpui_kit::test]
fn keyboard_cut_copy_paste_follow_the_menu_enablement(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(200, 150, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        drag_doc(window, (200, 150), (10.0, 10.0), (60.0, 60.0), cx);
        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);
        drag_doc(window, (200, 150), (100.0, 10.0), (150.0, 50.0), cx);
        window.render_frame(cx);
        let layers_before = content.read(cx).state_for_test().read(cx).layers.len();

        window.press("ctrl-c", cx);
        assert!(
            content.read(cx).clipboard_for_test().is_none(),
            "Ctrl+C copies nothing without an active-layer bitmap"
        );
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .status_error
                .is_none(),
            "Ctrl+C stays silent like the disabled menu row"
        );
        window.press("ctrl-x", cx);
        assert!(
            content.read(cx).clipboard_for_test().is_none(),
            "Ctrl+X copies nothing without an active-layer bitmap"
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layers.len(),
            layers_before,
            "Ctrl+X removes nothing"
        );
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .status_error
                .is_none(),
            "Ctrl+X stays silent like the disabled menu row"
        );
        window.press("ctrl-v", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layers.len(),
            layers_before,
            "Ctrl+V pastes nothing without a clipboard"
        );
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .status_error
                .is_none(),
            "Ctrl+V stays silent like the disabled menu row"
        );

        let (rows, _) = layer_rows(&content, cx);
        let background = rows
            .iter()
            .find(|row| row.name == "Background")
            .expect("the fixture Background row exists")
            .id;
        select_layer(&content, cx, background);
        window.render_frame(cx);
        window.press("ctrl-c", cx);
        assert!(
            content.read(cx).clipboard_for_test().is_some(),
            "Ctrl+C copies once the active layer owns pixels"
        );
    })
    .expect("test window remains open");
}

/// The remaining chords with menu-side enablement must match the observable
/// observable behaviour: at the history boundary and without a selection the
/// command is a silent no-op, not an error in the status bar. The Selection
/// drag establishes canvas focus, the same route the other chord tests use.
#[gpui_kit::test]
fn keyboard_undo_redo_and_deselect_no_op_silently_at_their_boundaries(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(200, 150, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        drag_doc(window, (200, 150), (10.0, 10.0), (60.0, 60.0), cx);
        assert!(
            content.read(cx).pixel_selection_present(),
            "the drag commits a selection and focuses the canvas"
        );

        // The enabled route still works.
        window.press("ctrl-d", cx);
        assert!(
            !content.read(cx).pixel_selection_present(),
            "Ctrl+D clears the committed selection"
        );
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .status_error
                .is_none(),
            "the enabled deselect is silent"
        );
        // Deselect without a selection.
        window.press("ctrl-d", cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .status_error
                .is_none(),
            "deselect without a selection stays silent"
        );

        content.update(cx, |app, cx| {
            app.nudge_selected_layer(10.0, 0.0, cx).expect("nudge");
        });
        window.render_frame(cx);
        let nudged = content
            .read(cx)
            .selected_transform_for_test(cx)
            .expect("selected layer");
        window.press("ctrl-z", cx);
        window.render_frame(cx);
        let undone = content
            .read(cx)
            .selected_transform_for_test(cx)
            .expect("selected layer");
        assert!(
            (undone.0 - (nudged.0 - 10.0)).abs() < 1e-6,
            "Ctrl+Z undoes the nudge"
        );
        window.press("ctrl-shift-z", cx);
        window.render_frame(cx);
        let redone = content
            .read(cx)
            .selected_transform_for_test(cx)
            .expect("selected layer");
        assert!(
            (redone.0 - nudged.0).abs() < 1e-6,
            "Ctrl+Shift+Z redoes the nudge"
        );
        // Redo again at the end of the history stream.
        window.press("ctrl-shift-z", cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .status_error
                .is_none(),
            "redo at the history boundary stays silent"
        );
        let after = content
            .read(cx)
            .selected_transform_for_test(cx)
            .expect("selected layer");
        assert!(
            (after.0 - nudged.0).abs() < 1e-6,
            "the silent redo leaves the transform unchanged"
        );
    })
    .expect("test window remains open");
}

/// The live canvas menu uses the 210 px context shell: a row is
/// 208 px wide (210 minus the two 1 px borders) and 28 px tall, matching the
/// captured menu box (442,462 210x280) and its 28 px rows.
#[gpui_kit::test]
fn canvas_context_menu_uses_the_reference_210_shell(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(200, 150, cx).expect("blank document");
        });
        window.render_frame(cx);
        open_canvas_menu(window, (200, 150), (100.0, 75.0), cx);
        let size = window.find("ctx-cut").bounds().size;
        let width = f32::from(size.width);
        let height = f32::from(size.height);
        assert!(
            (width - 208.0).abs() <= 1.0,
            "context row width is {width}, expected 208 (210 minus borders)"
        );
        assert!(
            (height - 28.0).abs() <= 1.0,
            "context row height is {height}, expected 28 (captured row metric)"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn canvas_menu_row_set_matches_the_reference(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(200, 150, cx).expect("blank document");
        });
        window.render_frame(cx);
        open_canvas_menu(window, (200, 150), (100.0, 5.0), cx);
        for id in [
            "ctx-cut",
            "ctx-copy",
            "ctx-paste",
            "ctx-select-all",
            "ctx-deselect",
            "ctx-invert",
            "ctx-actual-size",
            "ctx-fit-canvas",
            "ctx-zoom-selection",
        ] {
            assert!(window.try_find(id).is_some(), "missing context row {id}");
        }
        for id in ["ctx-undo", "ctx-redo", "ctx-duplicate", "ctx-flatten"] {
            assert!(window.try_find(id).is_none(), "stale row {id}");
        }
    })
    .expect("test window remains open");
}

/// Performance evidence: right-click dispatch through the rendered frame
/// that mounts the menu, five alternating closed/open cycles.
#[gpui_kit::test]
fn canvas_menu_open_frame_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(200, 150, cx).expect("blank document");
        });
        window.render_frame(cx);
        let mut samples = Vec::new();
        for _ in 0..5 {
            content.update(cx, |app, cx| {
                app.state_for_test()
                    .update(cx, |state, _| state.open_menu = None);
            });
            window.render_frame(cx);
            let (bounds, placement) = page(window, (200, 150));
            let position = window_point(bounds, placement, (200, 150), 100.0, 40.0);
            let start = std::time::Instant::now();
            let _ = window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Right,
                    position,
                    ..Default::default()
                }),
                cx,
            );
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert!(window.try_find("ctx-paste").is_some(), "menu must mount");
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "MENU_OPEN_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[2], sorted[4]
        );
    })
    .expect("test window remains open");
}

/// Performance evidence: click on the menubar Edit item through the
/// rendered frame that mounts its menu, five alternating closed/open cycles.
#[gpui_kit::test]
fn edit_menu_open_frame_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(200, 150, cx).expect("blank document");
        });
        window.render_frame(cx);
        let mut samples = Vec::new();
        for _ in 0..9 {
            content.update(cx, |app, cx| {
                app.state_for_test()
                    .update(cx, |state, _| state.open_menu = None);
            });
            window.render_frame(cx);
            let position = window.find("menus.edit").bounds().center();
            let start = std::time::Instant::now();
            let _ = window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position,
                    ..Default::default()
                }),
                cx,
            );
            let _ = window.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position,
                    ..Default::default()
                }),
                cx,
            );
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert!(
                window.try_find("menu-edit-undo").is_some(),
                "the Edit menu must mount"
            );
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "EDIT_MENU_OPEN_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
    })
    .expect("test window remains open");
}

/// The Move bar's numeric fields keep the reference widths - 62 px
/// for X/Y and 58 px for rotation - rather than being squeezed by the
/// flex row.
#[gpui_kit::test]
fn move_field_widths_match_the_reference(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(200, 150, cx).expect("blank document");
        });
        set_tool(&content, cx, crate::state::ToolId::Move, 1.0);
        window.render_frame(cx);
        for (id, expected) in [("num-x", 62.0), ("num-y", 62.0), ("num-r", 58.0)] {
            let width = window
                .try_find(id)
                .unwrap_or_else(|| panic!("{id} mounts in the Move bar"))
                .bounds()
                .size
                .width;
            assert!(
                (f32::from(width) - expected).abs() < 0.5,
                "{id} is {width}, expected {expected}"
            );
        }
    })
    .expect("test window remains open");
}

/// The File menu's recent-project rows are live actions, not labels: the row's
/// id, its ordering, the click that re-opens the file and the menu closing with
/// its rows are all asserted in one mounted frame. The recorded project's tab
/// is closed first, so the click has a job that nothing else in the frame does:
/// with `open_recent_path` neutered the active tab stays where it was and this
/// fails on the re-open assertion below.
#[gpui_kit::test]
fn file_menu_recent_row_opens_the_recorded_project(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            super::documents::open_second_document(app, cx, &directory);
            assert_eq!(
                app.active_tab_id_for_test().as_deref(),
                Some("multi-doc"),
                "the recent row points at the project just opened"
            );
            assert!(
                app.close_document_tab("multi-doc", cx),
                "the recorded project is clean, so closing it asks nothing"
            );
            assert_eq!(
                app.active_tab_id_for_test().as_deref(),
                Some("parity-doc"),
                "the first project is the one left active"
            );
            app.state_for_test().update(cx, |state, _| {
                state.open_menu = Some(crate::state::MenuId::File)
            });
        });
        window.render_frame(cx);
        assert!(
            window.try_find("recent-file-0").is_some(),
            "the File menu lists the recorded project"
        );
        window.click("recent-file-0", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            assert_eq!(
                app.active_tab_id_for_test().as_deref(),
                Some("multi-doc"),
                "the recent row re-opens the recorded project"
            );
            let state = app.state_for_test().read(cx);
            assert!(
                state.open_menu.is_none(),
                "the recent row closes the File menu"
            );
        });
        assert!(
            window.try_find("recent-file-0").is_none(),
            "the closed File menu unmounts its rows"
        );
    })
    .expect("test window remains open");
}
