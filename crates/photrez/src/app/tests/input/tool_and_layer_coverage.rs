//! Coverage across the tool set and the layer operations that follow it: every tool
//! round-trips, paint targets the selected layer, locked and non-raster layers refuse,
//! and reorder, duplicate, merge and flatten each undo.

use super::*;

#[gpui_kit::test]
fn all_ten_tools_round_trip_in_both_directions(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        // `g` selects the bucket and `Shift+G` the gradient, matching the
        // the canvas key handler; the ring can therefore run in reverse.
        let ring = [
            ("v", ToolId::Move),
            ("m", ToolId::Selection),
            ("c", ToolId::Crop),
            ("i", ToolId::Eyedropper),
            ("b", ToolId::Brush),
            ("e", ToolId::Eraser),
            ("g", ToolId::PaintBucket),
            ("shift-g", ToolId::Gradient),
            ("u", ToolId::Shape),
            ("t", ToolId::Text),
        ];
        for (key, tool) in ring {
            window.press(key, cx);
            content.update(cx, |app, cx| {
                assert_eq!(app.state_for_test().read(cx).active_tool, tool, "key {key}");
                assert_eq!(
                    app.input_phase_for_test(),
                    crate::input::GesturePhase::Idle,
                    "key {key} must not leave a gesture"
                );
            });
        }
        let reverse = [
            ("t", ToolId::Text),
            ("u", ToolId::Shape),
            ("shift-g", ToolId::Gradient),
            ("g", ToolId::PaintBucket),
            ("e", ToolId::Eraser),
            ("b", ToolId::Brush),
            ("i", ToolId::Eyedropper),
            ("c", ToolId::Crop),
            ("m", ToolId::Selection),
            ("v", ToolId::Move),
        ];
        for (key, tool) in reverse {
            window.press(key, cx);
            content.update(cx, |app, cx| {
                assert_eq!(app.state_for_test().read(cx).active_tool, tool, "key {key}");
            });
        }
        // A->B->A for every adjacent pair has now run twice; the canvas still
        // paints after the round trip.
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Brush;
                state.brush_size = 2.0;
            });
        });
        let (w, h, before) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (1.5, 0.5), cx);
        assert_ne!(
            pixels(&content, cx).2,
            before,
            "painting must still work after the full tool round trip"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn paint_targets_the_selected_layer_not_the_first_raster(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        let (rows, selected) = layer_rows(&content, cx);
        assert_eq!(rows.len(), 2, "the panel must project both core layers");
        assert_eq!(rows[0].name, "top");
        assert_eq!(rows[0].core_id.as_deref(), Some("top"));
        assert_eq!(rows[1].name, "bottom");
        assert_eq!(rows[1].core_id.as_deref(), Some("bottom"));
        assert_eq!(selected, rows[0].id, "projection selects the first row");

        let (_, _, top_before) = content
            .read(cx)
            .core_layer_pixels_for_test("top")
            .expect("top layer has pixels");
        let (_, _, bottom_before) = content
            .read(cx)
            .core_layer_pixels_for_test("bottom")
            .expect("bottom layer has pixels");
        let cursor_before = history_cursor(&content, cx);

        select_layer(&content, cx, rows[1].id);
        drag_doc(window, (2, 1), (0.5, 0.5), (1.5, 0.5), cx);
        let (_, _, top_after) = content.read(cx).core_layer_pixels_for_test("top").unwrap();
        let (_, _, bottom_after) = content
            .read(cx)
            .core_layer_pixels_for_test("bottom")
            .unwrap();
        assert_eq!(
            top_after, top_before,
            "the unselected top layer must not change"
        );
        assert_ne!(
            bottom_after, bottom_before,
            "the selected bottom layer must paint"
        );
        assert_eq!(history_cursor(&content, cx), cursor_before + 1);

        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        let (_, _, bottom_undone) = content
            .read(cx)
            .core_layer_pixels_for_test("bottom")
            .unwrap();
        assert_eq!(
            bottom_undone, bottom_before,
            "undo restores the selected layer"
        );

        select_layer(&content, cx, rows[0].id);
        drag_doc(window, (2, 1), (0.5, 0.5), (1.5, 0.5), cx);
        let (_, _, top_after_top) = content.read(cx).core_layer_pixels_for_test("top").unwrap();
        let (_, _, bottom_after_top) = content
            .read(cx)
            .core_layer_pixels_for_test("bottom")
            .unwrap();
        assert_ne!(top_after_top, top_before, "selecting top paints the top");
        assert_eq!(
            bottom_after_top, bottom_before,
            "the deselected bottom layer must not change"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn locked_selected_layer_refuses_brush_and_bucket(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(true),
    );
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        let (rows, selected) = layer_rows(&content, cx);
        assert_eq!(selected, rows[0].id, "the locked top row is selected");
        let (_, _, top_before) = content.read(cx).core_layer_pixels_for_test("top").unwrap();
        let (_, _, bottom_before) = content
            .read(cx)
            .core_layer_pixels_for_test("bottom")
            .unwrap();
        let cursor_before = history_cursor(&content, cx);
        drag_doc(window, (2, 1), (0.5, 0.5), (1.5, 0.5), cx);
        set_tool(&content, cx, ToolId::PaintBucket, 1.0);
        click_doc(window, (2, 1), (0.5, 0.5), cx);
        let (_, _, top_after) = content.read(cx).core_layer_pixels_for_test("top").unwrap();
        let (_, _, bottom_after) = content
            .read(cx)
            .core_layer_pixels_for_test("bottom")
            .unwrap();
        assert_eq!(top_after, top_before, "a locked layer must refuse paint");
        assert_eq!(bottom_after, bottom_before);
        assert_eq!(
            history_cursor(&content, cx),
            cursor_before,
            "a refused paint must add no history"
        );
        assert_eq!(
            content.read(cx).input_phase_for_test(),
            crate::input::GesturePhase::Idle
        );

        select_layer(&content, cx, rows[1].id);
        set_tool(&content, cx, ToolId::Brush, 2.0);
        drag_doc(window, (2, 1), (0.5, 0.5), (1.5, 0.5), cx);
        let (_, _, bottom_unlocked) = content
            .read(cx)
            .core_layer_pixels_for_test("bottom")
            .unwrap();
        assert_ne!(
            bottom_unlocked, bottom_before,
            "an unlocked selected layer still paints"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn non_raster_selected_layer_refuses_paint(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let doc = (w, h);
        drag_doc(window, doc, (0.2, 0.2), (1.8, 0.8), cx);
        let (rows, _) = layer_rows(&content, cx);
        assert_eq!(rows.len(), 2, "the shape layer projects into the rows");
        assert!(
            rows[0]
                .core_id
                .as_deref()
                .unwrap_or("")
                .starts_with("native-layer-"),
            "the shape is the top row"
        );

        set_tool(&content, cx, ToolId::Brush, 2.0);
        select_layer(&content, cx, rows[0].id);
        let cursor_before = history_cursor(&content, cx);
        let raster_id = rows[1].core_id.as_deref().unwrap();
        let (_, _, raster_before) = content
            .read(cx)
            .core_layer_pixels_for_test(raster_id)
            .unwrap();
        drag_doc(window, doc, (0.5, 0.5), (1.5, 0.5), cx);
        let (_, _, raster_after) = content
            .read(cx)
            .core_layer_pixels_for_test(raster_id)
            .unwrap();
        assert_eq!(
            raster_after, raster_before,
            "painting a shape layer must not fall through to the raster below"
        );
        assert_eq!(history_cursor(&content, cx), cursor_before);

        select_layer(&content, cx, rows[1].id);
        drag_doc(window, doc, (0.5, 0.5), (1.5, 0.5), cx);
        let (_, _, raster_selected) = content
            .read(cx)
            .core_layer_pixels_for_test(raster_id)
            .unwrap();
        assert_ne!(
            raster_selected, raster_before,
            "selecting the raster row restores painting"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn layer_drag_reorders_core_and_undo_restores(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_core_layer_ids_for_test(),
            vec!["top", "bottom"]
        );
        window.drag_to("layer-row-1", "layer-row-0", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_core_layer_ids_for_test(),
            vec!["bottom", "top"],
            "the drag must reorder the canonical layers"
        );
        let (rows, selected) = layer_rows(&content, cx);
        assert_eq!(rows[0].name, "bottom");
        assert_eq!(selected, rows[0].id, "the dragged layer must stay selected");

        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_core_layer_ids_for_test(),
            vec!["top", "bottom"],
            "undo must restore the canonical order"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn window_close_prompts_when_dirty_and_cancel_keeps_the_document(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (1.5, 0.5), cx);
        assert!(
            content
                .read(cx)
                .active_document_save_state_for_test()
                .expect("fixture has a session")
                .2
        );
    })
    .expect("test window remains open");

    let mut visual = gpui_kit::VisualTestContext::from_window(window.into(), cx);
    assert!(
        !visual.simulate_close(),
        "a dirty close must be interrupted by the prompt"
    );
    visual.update(|window, cx| {
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            Some(crate::state::DialogId::DirtyClose)
        );
        window.render_frame(cx);
        window.click("dirty-close-cancel", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            None
        );
        assert!(
            content
                .read(cx)
                .active_document_save_state_for_test()
                .expect("the document stays open")
                .2,
            "cancel must keep the document dirty"
        );
    });
}

#[gpui_kit::test]
fn window_close_discard_closes_the_document_and_drops_the_prompt(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (1.5, 0.5), cx);
    })
    .expect("test window remains open");

    let mut visual = gpui_kit::VisualTestContext::from_window(window.into(), cx);
    assert!(!visual.simulate_close(), "the dirty prompt must open");
    visual.update(|window, cx| {
        window.render_frame(cx);
        window.click("dirty-close-discard", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert!(!state.has_document, "discard closes the document");
        assert_eq!(state.active_dialog, None);
    });
}

#[gpui_kit::test]
fn duplicate_selected_layer_copies_pixels_and_undoes(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (_, _, before_pixels) = content.read(cx).core_layer_pixels_for_test("top").unwrap();
        content.update(cx, |app, cx| {
            app.duplicate_selected_layer(cx)
                .expect("duplicate succeeds");
        });
        let ids = content.read(cx).active_core_layer_ids_for_test();
        assert_eq!(ids.len(), 3, "the duplicate must be a new layer");
        let duplicate_id = ids
            .iter()
            .find(|id| id.starts_with("native-layer-"))
            .expect("the duplicate has a generated id")
            .clone();
        let (_, _, pixels) = content
            .read(cx)
            .core_layer_pixels_for_test(&duplicate_id)
            .expect("the duplicate has pixels");
        assert_eq!(pixels, before_pixels, "the duplicate copies the pixels");
        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        assert_eq!(
            content.read(cx).active_core_layer_ids_for_test().len(),
            2,
            "undo removes the duplicate"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn merge_down_composites_pixels_and_undoes(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            app.merge_selected_layer_down(cx)
                .expect("merge down succeeds");
        });
        let ids = content.read(cx).active_core_layer_ids_for_test();
        assert_eq!(ids.len(), 1, "the pair becomes one layer");
        let (width, height, pixels) = content
            .read(cx)
            .core_layer_pixels_for_test(&ids[0])
            .expect("the merged layer has pixels");
        assert_eq!((width, height), (2, 1));
        assert_eq!(&pixels[0..4], &[0, 0, 255, 255], "top wins at (0,0)");
        assert_eq!(&pixels[4..8], &[255, 0, 0, 255], "bottom remains at (1,0)");

        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        let ids = content.read(cx).active_core_layer_ids_for_test();
        assert_eq!(ids, vec!["top", "bottom"], "undo restores the pair");
        assert!(content.read(cx).core_layer_pixels_for_test("top").is_some());
        assert!(content
            .read(cx)
            .core_layer_pixels_for_test("bottom")
            .is_some());

        content.update(cx, |app, cx| {
            app.step_history(false, cx).expect("redo succeeds");
        });
        assert_eq!(
            content.read(cx).active_core_layer_ids_for_test().len(),
            1,
            "redo restores the merged layer"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn flatten_composites_pixels_and_undoes(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            app.flatten_document(cx).expect("flatten succeeds");
        });
        let ids = content.read(cx).active_core_layer_ids_for_test();
        assert_eq!(ids.len(), 1, "flatten leaves one layer");
        let (_, _, pixels) = content
            .read(cx)
            .core_layer_pixels_for_test(&ids[0])
            .expect("the flattened layer has pixels");
        assert_eq!(&pixels[0..4], &[0, 0, 255, 255]);
        assert_eq!(&pixels[4..8], &[255, 0, 0, 255]);
        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        assert_eq!(
            content.read(cx).active_core_layer_ids_for_test(),
            vec!["top", "bottom"],
            "undo restores the flattened layers"
        );
    })
    .expect("test window remains open");
}
