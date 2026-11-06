use super::canvas_input::{mount_archive_app, select_layer};
use super::*;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{MouseButton, MouseDownEvent, MouseUpEvent, PlatformInput};

pub(super) fn layers_len(content: &Entity<App>, cx: &mut gpui_kit::App) -> usize {
    content.read(cx).active_core_layer_count_for_test().unwrap()
}

pub(super) fn flags(
    content: &Entity<App>,
    cx: &mut gpui_kit::App,
) -> Vec<crate::state::LayerFlags> {
    content
        .read(cx)
        .state_for_test()
        .read(cx)
        .layer_flags
        .clone()
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
        assert!(
            window.try_find("layer-eye-0-tooltip").is_none(),
            "the eye tooltip is absent at rest"
        );
        window.hover("layer-eye-0", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("layer-eye-0-tooltip").is_some(),
            "hovering the eye shows its tooltip"
        );
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
        let (rows, _) = super::canvas_input::layer_rows(&content, cx);
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
fn properties_transform_status_follows_the_layer_locks(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (rows, _) = super::canvas_input::layer_rows(&content, cx);
        let top = rows
            .iter()
            .find(|row| row.core_id.as_deref() == Some("top"))
            .expect("top row")
            .id;
        select_layer(&content, cx, top);
        window.render_frame(cx);
        assert!(
            window.try_find("properties-status-pos").is_none()
                && window.try_find("properties-status-rot").is_none()
                && window.try_find("properties-status-pos-rot").is_none()
                && window.try_find("properties-status-layer-locked").is_none(),
            "an unlocked layer shows no transform status hint"
        );
        window.click("layer-lock-header-position", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("properties-status-pos").is_some(),
            "a position lock shows the position hint"
        );
        window.click("layer-lock-header-rotation", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("properties-status-pos-rot").is_some(),
            "both partial locks show the combined hint"
        );
        window.click("layer-lock-header-base", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("properties-status-layer-locked").is_some(),
            "the base lock outranks the partial hints"
        );
        window.click("layer-lock-header-base", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("properties-status-pos-rot").is_some(),
            "unlocking the base lock returns to the partial hint"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn row_lock_never_toggles_the_background_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with_background(),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let rows_and_flags = flags(&content, cx);
        assert!(
            rows_and_flags[1].is_background,
            "the fixture bottom row is the Background"
        );
        let (rows, _) = super::canvas_input::layer_rows(&content, cx);
        let bottom = rows
            .iter()
            .find(|row| row.core_id.as_deref() == Some("bottom"))
            .expect("bottom row")
            .id;
        window.click(format!("layer-lock-{bottom}"), cx);
        window.render_frame(cx);
        assert!(
            !flags(&content, cx)[1].locked,
            "the Background lock never writes a lock flag"
        );
        assert!(
            !content
                .read(cx)
                .active_document_save_state_for_test()
                .is_some_and(|(_, _, dirty)| dirty),
            "the guarded click leaves the document untouched"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn background_lock_click_shows_the_warn_toast(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with_background(),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("toast-1").is_none(),
            "the live host is empty at rest"
        );
        let (rows, _) = super::canvas_input::layer_rows(&content, cx);
        let bottom = rows
            .iter()
            .find(|row| row.core_id.as_deref() == Some("bottom"))
            .expect("bottom row")
            .id;
        window.click(format!("layer-lock-{bottom}"), cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.toasts.len(), 1, "the guarded click posts one toast");
            assert_eq!(state.toasts[0].message, "Rename the layer to unlock");
            assert_eq!(state.toasts[0].severity, crate::state::ToastSeverity::Warn);
        }
        assert!(
            window.try_find("toast-1").is_some(),
            "the host paints the live queue"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn live_toasts_cap_at_three_and_dismiss_by_id(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            for index in 0..4 {
                app.show_toast(
                    format!("toast {index}"),
                    crate::state::ToastSeverity::Info,
                    cx,
                );
            }
        });
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.toasts.len(), 3, "the queue keeps the last three");
            assert_eq!(
                state.toasts[0].message, "toast 1",
                "the oldest toast is dropped"
            );
            assert_eq!(state.next_toast_id, 5);
        }
        assert!(
            window.try_find("toast-1").is_none(),
            "the dropped row is gone"
        );
        assert!(
            window.try_find("toast-4").is_some(),
            "the newest row paints"
        );

        content.update(cx, |app, cx| app.dismiss_toast(3, cx));
        window.render_frame(cx);
        assert!(
            window.try_find("toast-3").is_none(),
            "dismiss removes the row by id"
        );
        assert!(window.try_find("toast-2").is_some(), "siblings survive");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn header_lock_toggles_drive_core_and_disable_the_partial_locks(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (rows, _) = super::canvas_input::layer_rows(&content, cx);
        let top = rows
            .iter()
            .find(|row| row.core_id.as_deref() == Some("top"))
            .expect("top row")
            .id;
        select_layer(&content, cx, top);
        window.render_frame(cx);

        window.click("layer-lock-header-base", cx);
        window.render_frame(cx);
        assert!(
            flags(&content, cx)[0].locked,
            "the header's base toggle locks the selected layer"
        );

        window.click("layer-lock-header-transparency", cx);
        window.render_frame(cx);
        assert!(
            !flags(&content, cx)[0].lock_transparency,
            "a base lock disables the partial toggles"
        );

        window.click("layer-lock-header-base", cx);
        window.render_frame(cx);
        assert!(!flags(&content, cx)[0].locked, "the base toggle unlocks");
        window.click("layer-lock-header-transparency", cx);
        window.render_frame(cx);
        assert!(
            flags(&content, cx)[0].lock_transparency,
            "the transparency toggle flips its own flag"
        );

        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        assert!(
            !flags(&content, cx)[0].lock_transparency,
            "undo restores the transparency flag"
        );

        window.render_frame(cx);
        assert!(
            window.try_find("layer-lock-header-base-tooltip").is_none(),
            "the lock tooltip is absent at rest"
        );
        window.hover("layer-lock-header-base", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("layer-lock-header-base-tooltip").is_some(),
            "hovering the base toggle shows its tooltip"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn row_chevrons_reorder_through_core_and_stop_at_the_ends(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    // Row ids are projection positions and are reassigned on every
    // reprojection, so order is asserted through `core_id`, which is stable.
    let core_ids = |content: &Entity<App>, cx: &mut gpui_kit::App| -> Vec<String> {
        super::canvas_input::layer_rows(content, cx)
            .0
            .into_iter()
            .map(|row| row.core_id.expect("the fixture layers have core ids"))
            .collect()
    };
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (rows, _) = super::canvas_input::layer_rows(&content, cx);
        assert_eq!(rows.len(), 2, "the fixture opens two layers");
        assert_eq!(
            rows[0].core_id.as_deref(),
            Some("top"),
            "the projection lists the top layer first"
        );
        let top = rows[0].id;
        let bottom = rows[1].id;

        window.click(format!("layer-up-{bottom}"), cx);
        window.render_frame(cx);
        assert_eq!(
            core_ids(&content, cx),
            ["bottom", "top"],
            "the second row moves above the first"
        );

        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        window.render_frame(cx);
        assert_eq!(
            core_ids(&content, cx),
            ["top", "bottom"],
            "undo restores the core order"
        );

        window.click(format!("layer-up-{top}"), cx);
        window.click(format!("layer-down-{bottom}"), cx);
        window.render_frame(cx);
        assert_eq!(
            core_ids(&content, cx),
            ["top", "bottom"],
            "the end chevrons cannot reorder"
        );

        assert!(
            window
                .try_find(format!("layer-up-{bottom}-tooltip"))
                .is_none(),
            "the chevron is tooltip-free until hovered"
        );
        window.hover(format!("layer-up-{bottom}"), cx);
        window.render_frame(cx);
        assert!(
            window
                .try_find(format!("layer-up-{bottom}-tooltip"))
                .is_some(),
            "hovering the chevron shows the move-layer tooltip"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn adjustments_indicator_follows_the_core_flag(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with_adjusted_top(),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (rows, _) = super::canvas_input::layer_rows(&content, cx);
        assert_eq!(rows.len(), 2);
        assert!(
            window
                .try_find(format!("layer-adjustments-{}", rows[0].id))
                .is_some(),
            "the adjusted top layer shows the indicator"
        );
        assert!(
            window
                .try_find(format!("layer-adjustments-{}", rows[1].id))
                .is_none(),
            "a layer with no adjustments shows nothing"
        );
        assert!(
            window
                .try_find(format!("layer-adjustments-{}-tooltip", rows[0].id))
                .is_none(),
            "the indicator is tooltip-free until hovered"
        );
        window.hover(format!("layer-adjustments-{}", rows[0].id), cx);
        window.render_frame(cx);
        assert!(
            window
                .try_find(format!("layer-adjustments-{}-tooltip", rows[0].id))
                .is_some(),
            "hovering the indicator explains what it marks"
        );
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
fn blend_cycle_reaches_difference_and_undoes_stepwise(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layer_blend,
            "Normal"
        );
        let expected = [
            "Multiply",
            "Screen",
            "Overlay",
            "Darken",
            "Lighten",
            "Color Dodge",
            "Color Burn",
            "Soft Light",
            "Hard Light",
            "Difference",
        ];
        for (step, mode) in expected.iter().enumerate() {
            window.click("layer-blend-cycle", cx);
            window.render_frame(cx);
            assert_eq!(
                content.read(cx).state_for_test().read(cx).layer_blend,
                *mode,
                "click {} advances the cycle",
                step + 1
            );
        }
        for _ in 0..10 {
            content.update(cx, |app, cx| {
                app.step_history(true, cx).expect("undo blend");
            });
        }
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layer_blend,
            "Normal",
            "each cycle click is its own undo step"
        );
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
        assert!(window.find("rename-value").bounds().size.width > gpui_kit::px(0.0));
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
fn tab_reaches_the_rename_field_and_types_into_it(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("layer-rename-0", cx);
        window.render_frame(cx);

        let mut reached = false;
        for _ in 0..40 {
            window.press("tab", cx);
            window.render_frame(cx);
            if window.find("rename-value").focused() == Some(true) {
                reached = true;
                break;
            }
        }
        assert!(reached, "Tab must reach the Rename dialog's field");

        window.press("y", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).rename_content_for_test(cx).as_deref(),
            Some("Rastery"),
            "the keystroke followed the focused field"
        );
        window.press("escape", cx);
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
        super::canvas_input::drag_doc(window, (2, 1), (0.1, 0.1), (0.9, 0.9), cx);
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
        let (rows, _) = super::canvas_input::layer_rows(&content, cx);
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
        let (rows, _) = super::canvas_input::layer_rows(&content, cx);
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

/// The locked badge follows the layer's real lock flag, not a constant.
#[gpui_kit::test]
fn locked_rows_carry_the_badge_and_unlocked_rows_do_not(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (rows, _) = super::canvas_input::layer_rows(&content, cx);
        let top = rows
            .iter()
            .find(|row| row.core_id.as_deref() == Some("top"))
            .expect("top row")
            .id;
        assert!(
            !flags(&content, cx)[0].locked,
            "the fixture row starts unlocked"
        );
        assert!(
            window
                .try_find(format!("layer-locked-badge-{top}"))
                .is_none(),
            "an unlocked row shows no badge"
        );

        window.click(format!("layer-lock-{top}"), cx);
        window.render_frame(cx);
        assert!(
            flags(&content, cx)[0].locked,
            "the row lock button locks the layer"
        );
        let badge = window
            .try_find(format!("layer-locked-badge-{top}"))
            .expect("a locked row shows the badge");
        assert!(
            badge.bounds().size.width > gpui_kit::px(0.0),
            "the badge takes its own row slot"
        );

        window.click(format!("layer-lock-{top}"), cx);
        window.render_frame(cx);
        assert!(
            !flags(&content, cx)[0].locked,
            "the second click unlocks again"
        );
        assert!(
            window
                .try_find(format!("layer-locked-badge-{top}"))
                .is_none(),
            "unlocking takes the badge away"
        );
    })
    .expect("test window remains open");
}

/// The drag ghost carries the layer tile only while a row is actually dragged.
#[gpui_kit::test]
fn layer_drag_ghost_carries_the_icon_tile(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("layer-drag-ghost-tile").is_none(),
            "nothing is dragged, so the tile is absent"
        );
        let (rows, _) = super::canvas_input::layer_rows(&content, cx);
        let start = window
            .find(format!("layer-row-{}", rows[0].id))
            .bounds()
            .center();
        // Release over the canvas: no row is under the pointer, so the drag
        // ends without a reorder.
        let moved = gpui_kit::point(start.x - gpui_kit::px(260.0), start.y);
        let _ = window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        let _ = window.dispatch_event(
            PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        assert!(cx.has_active_drag(), "the layer row drag is active");
        assert!(
            window.try_find("layer-drag-ghost-tile").is_some(),
            "the floating ghost carries the layer tile"
        );
        let _ = window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        assert!(
            window.try_find("layer-drag-ghost-tile").is_none(),
            "the tile goes away when the drag ends"
        );
        assert_eq!(
            super::canvas_input::layer_rows(&content, cx).0.len(),
            2,
            "releasing over the canvas reorders nothing"
        );
    })
    .expect("test window remains open");
}
