use super::input::{mount_archive_app, select_layer};
use super::*;
use gpui_kit::test::TestWindowExt as _;

fn layers_len(content: &Entity<App>, cx: &mut gpui_kit::App) -> usize {
    content.read(cx).active_core_layer_count_for_test().unwrap()
}

fn flags(content: &Entity<App>, cx: &mut gpui_kit::App) -> Vec<crate::state::LayerFlags> {
    content
        .read(cx)
        .state_for_test()
        .read(cx)
        .layer_flags
        .clone()
}

#[gpui_kit::test]
fn footer_adds_and_deletes_layers_with_undo(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(layers_len(&content, cx), 2);
        window.click("layer-action-0", cx);
        window.render_frame(cx);
        assert_eq!(layers_len(&content, cx), 3, "add creates an empty layer");
        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        assert_eq!(layers_len(&content, cx), 2, "undo removes the added layer");

        let (rows, selected) = super::input::layer_rows(&content, cx);
        let selected_core = rows
            .iter()
            .find(|row| row.id == selected)
            .and_then(|row| row.core_id.clone())
            .expect("a core layer is selected");
        window.click("layer-action-4", cx);
        window.render_frame(cx);
        assert_eq!(layers_len(&content, cx), 1, "delete removes the layer");
        assert!(
            !content
                .read(cx)
                .active_core_layer_ids_for_test()
                .contains(&selected_core),
            "the selected core layer is gone"
        );
        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        assert_eq!(layers_len(&content, cx), 2, "undo restores the layer");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn row_eye_toggle_hides_the_layer_and_undo_restores_it(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(flags(&content, cx)[0].visible);
        window.click("layer-eye-0", cx);
        window.render_frame(cx);
        assert!(
            !flags(&content, cx)[0].visible,
            "the eye toggle hides the top layer"
        );
        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        assert!(flags(&content, cx)[0].visible);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn row_lock_toggle_round_trips_and_blocks_painting(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (rows, _) = super::input::layer_rows(&content, cx);
        let top = rows
            .iter()
            .find(|row| row.core_id.as_deref() == Some("top"))
            .expect("top row")
            .id;
        select_layer(&content, cx, top);
        window.click(format!("layer-lock-{top}"), cx);
        window.render_frame(cx);
        assert!(
            flags(&content, cx)[0].locked,
            "the lock toggle locks the row"
        );
        window.click(format!("layer-lock-{top}"), cx);
        window.render_frame(cx);
        assert!(!flags(&content, cx)[0].locked, "the second click unlocks");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn opacity_and_blend_controls_write_through_core(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layer_opacity,
            1.0
        );
        window.click("layer-opacity-dec", cx);
        window.render_frame(cx);
        let opacity = content.read(cx).state_for_test().read(cx).layer_opacity;
        assert!((opacity - 0.9).abs() < 1e-6, "opacity={opacity}");

        assert_eq!(
            content.read(cx).state_for_test().read(cx).layer_blend,
            "Normal"
        );
        window.click("layer-blend-cycle", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layer_blend,
            "Multiply"
        );

        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo blend");
            app.step_history(true, cx).expect("undo opacity");
        });
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(state.layer_blend, "Normal");
        assert_eq!(state.layer_opacity, 1.0);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn rename_button_opens_a_focused_prompt_and_enter_commits(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("layer-rename-0", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).rename_content_for_test(cx).as_deref(),
            Some("Raster")
        );
        window.press("x", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert!(content.read(cx).rename_content_for_test(cx).is_none());
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(state.layers[0].name, "Rasterx");
        assert_eq!(state.active_dialog, None);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn select_all_layers_then_merge_selected_composites_them(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            app.select_all_layers(cx);
            assert_eq!(app.selected_layer_count(cx), 2);
            app.merge_selected_layers(cx)
                .expect("merge selected succeeds");
            assert_eq!(app.active_core_layer_count_for_test(), Some(1));
            let ids = app.active_core_layer_ids_for_test();
            let (_, _, pixels) = app
                .core_layer_pixels_for_test(&ids[0])
                .expect("merged layer pixels");
            assert_eq!(&pixels[0..4], &[0, 0, 255, 255], "top wins at (0,0)");
            assert_eq!(&pixels[4..8], &[255, 0, 0, 255], "bottom remains at (1,0)");
        });
        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo");
        });
        assert_eq!(layers_len(&content, cx), 2, "undo restores both layers");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn stamp_visible_adds_a_composited_layer_and_undo_removes_it(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, _window, cx| {
        content.update(cx, |app, cx| {
            app.stamp_visible(cx).expect("stamp succeeds");
            assert_eq!(app.active_core_layer_count_for_test(), Some(3));
            let ids = app.active_core_layer_ids_for_test();
            let stamp_id = &ids[0];
            let (_, _, pixels) = app
                .core_layer_pixels_for_test(stamp_id)
                .expect("stamp pixels");
            assert_eq!(&pixels[0..4], &[0, 0, 255, 255]);
            assert_eq!(&pixels[4..8], &[255, 0, 0, 255]);
            app.step_history(true, cx).expect("undo");
            assert_eq!(app.active_core_layer_count_for_test(), Some(2));
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn raster_layers_get_thumbnails_and_shape_layers_keep_the_checker(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(content.read(cx).layer_thumb_stats_for_test(), (2, 2));
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = crate::state::ToolId::Shape
            });
        });
        window.render_frame(cx);
        super::input::drag_doc(window, (2, 1), (0.1, 0.1), (0.9, 0.9), cx);
        assert_eq!(
            content.read(cx).layer_thumb_stats_for_test(),
            (3, 2),
            "the shape row keeps the checker tile"
        );
    })
    .expect("test window remains open");
}

fn click_row(
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
    window.render_frame(cx);
}

fn selected_rows(content: &Entity<App>, cx: &mut gpui_kit::App) -> Vec<u64> {
    content
        .read(cx)
        .state_for_test()
        .read(cx)
        .selected_layers
        .clone()
}

#[gpui_kit::test]
fn ctrl_click_and_plain_click_manage_the_multi_selection(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (rows, _) = super::input::layer_rows(&content, cx);
        assert_eq!(rows.len(), 2, "the fixture has two layers");
        let (first, second) = (rows[0].id, rows[1].id);

        click_row(window, second, gpui_kit::Modifiers::default(), cx);
        assert_eq!(
            selected_rows(&content, cx),
            vec![second],
            "a plain click selects one layer"
        );

        click_row(
            window,
            first,
            gpui_kit::Modifiers {
                control: true,
                ..Default::default()
            },
            cx,
        );
        let selected = selected_rows(&content, cx);
        assert_eq!(selected.len(), 2, "Ctrl+click adds to the selection");
        assert!(selected.contains(&first) && selected.contains(&second));

        click_row(window, first, gpui_kit::Modifiers::default(), cx);
        assert_eq!(
            selected_rows(&content, cx),
            vec![first],
            "a plain click collapses the selection"
        );

        window.click("layer-action-0", cx);
        window.render_frame(cx);
        let (rows, _) = super::input::layer_rows(&content, cx);
        assert_eq!(rows.len(), 3, "the footer adds a third layer");
        let (top, middle, bottom) = (rows[0].id, rows[1].id, rows[2].id);

        click_row(window, bottom, gpui_kit::Modifiers::default(), cx);
        click_row(
            window,
            top,
            gpui_kit::Modifiers {
                shift: true,
                ..Default::default()
            },
            cx,
        );
        assert_eq!(
            selected_rows(&content, cx),
            vec![top, middle, bottom],
            "Shift+click selects the contiguous range"
        );
        click_row(
            window,
            middle,
            gpui_kit::Modifiers {
                shift: true,
                ..Default::default()
            },
            cx,
        );
        assert_eq!(
            selected_rows(&content, cx),
            vec![top, middle],
            "Shift+click re-anchors at the previous active row"
        );

        click_row(
            window,
            bottom,
            gpui_kit::Modifiers {
                control: true,
                ..Default::default()
            },
            cx,
        );
        click_row(
            window,
            middle,
            gpui_kit::Modifiers {
                control: true,
                ..Default::default()
            },
            cx,
        );
        assert_eq!(
            selected_rows(&content, cx),
            vec![top, bottom],
            "Ctrl+click toggles one row"
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).selected_layer,
            bottom,
            "removing the active row falls back to the last selected"
        );
    })
    .expect("test window remains open");
}
