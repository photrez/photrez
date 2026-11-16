use super::canvas_input::{history_cursor, mount_archive_app, select_layer, set_tool};
use super::*;
use crate::state::{LayerTab, ToolId};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{MouseButton, MouseDownEvent, MouseUpEvent, PlatformInput, px};

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
fn new_layers_follow_photoshop_numbering(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(100, 100, cx).expect("blank document");
            app.add_empty_layer(cx);
        });
        window.render_frame(cx);
        let first = content
            .read(cx)
            .selected_layer_for_cursor(cx)
            .expect("the first new layer is selected")
            .1
            .name;
        assert_eq!(first, "Layer 1");
        content.update(cx, |app, cx| app.add_empty_layer(cx));
        window.render_frame(cx);
        let second = content
            .read(cx)
            .selected_layer_for_cursor(cx)
            .expect("the second new layer is selected")
            .1
            .name;
        assert_eq!(second, "Layer 2");
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
        assert!(
            window.try_find("layer-eye-0-tooltip").is_none(),
            "the eye tooltip is absent at rest"
        );
        window.hover("layer-eye-0", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("layer-eye-0-tooltip").is_none(),
            "the eye tooltip waits out the hover delay"
        );
    })
    .expect("test window remains open");
    settle_tooltip(cx, window, |window, cx| {
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
    });
}

/// The row's eye leads: it sits at the row's 14px leading inset, left of the
/// thumbnail, in a 24px button - the position the reference mounts it at -
/// instead of hanging off the row's trailing end.
///
/// RED proof: against the trailing-eye row this fails on the position assert,
/// which finds the eye half a row away.
#[gpui_kit::test]
fn row_eye_leads_the_row_at_the_leading_inset(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, _content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let row = window.find("layer-row-0").bounds();
        let eye = window.find("layer-eye-0").bounds();
        assert_eq!(
            eye.origin.x,
            row.origin.x + px(14.),
            "the eye is the row's first child at the leading inset"
        );
        assert_eq!(eye.size.width, px(24.), "the eye button is 24px wide");
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
            window.try_find("layer-lock-header-base-tooltip").is_none(),
            "the lock tooltip waits out the hover delay"
        );
    })
    .expect("test window remains open");
    settle_tooltip(cx, window, |window, _cx| {
        assert!(
            window.try_find("layer-lock-header-base-tooltip").is_some(),
            "hovering the base toggle shows its tooltip"
        );
    });
}

#[gpui_kit::test]
fn row_chevrons_reorder_through_core_and_stop_at_the_ends(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let chevron_bottom = std::cell::Cell::new(0u64);
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
        chevron_bottom.set(bottom);

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
                .is_none(),
            "the chevron tooltip waits out the hover delay"
        );
    })
    .expect("test window remains open");
    settle_tooltip(cx, window, |window, _cx| {
        let bottom = chevron_bottom.get();
        assert!(
            window
                .try_find(format!("layer-up-{bottom}-tooltip"))
                .is_some(),
            "hovering the chevron shows the move-layer tooltip"
        );
    });
}

#[gpui_kit::test]
fn adjustments_indicator_follows_the_core_flag(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let adjusted_top = std::cell::Cell::new(0u64);
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with_adjusted_top(),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (rows, _) = super::canvas_input::layer_rows(&content, cx);
        assert_eq!(rows.len(), 2);
        adjusted_top.set(rows[0].id);
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
                .is_none(),
            "the indicator tooltip waits out the hover delay"
        );
    })
    .expect("test window remains open");
    settle_tooltip(cx, window, |window, _cx| {
        let top = adjusted_top.get();
        assert!(
            window
                .try_find(format!("layer-adjustments-{}-tooltip", top))
                .is_some(),
            "hovering the indicator explains what it marks"
        );
    });
}

/// The blend dropdown's pick writes through core history: the panel opens on
/// the trigger, choosing Multiply applies it, and one undo returns Normal.
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
        window.click("layer-blend-option-1", cx);
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
fn blend_menu_picks_difference_and_undoes_in_one_step(cx: &mut gpui_kit::TestAppContext) {
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
        window.click("layer-blend-cycle", cx);
        window.render_frame(cx);
        // Difference is the eleventh entry of the dropdown's order.
        window.click("layer-blend-option-10", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layer_blend,
            "Difference",
            "the dropdown picks Difference directly"
        );
        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo blend");
        });
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layer_blend,
            "Normal",
            "one pick is one undo step"
        );
    })
    .expect("test window remains open");
}

/// A double click on the row's name opens a focused rename prompt and Enter
/// commits it - the reference starts a rename from the name, so the row
/// carries no pen button.
fn double_click_name(window: &mut gpui_kit::Window, row: u64, cx: &mut gpui_kit::App) {
    let at = window.find(format!("layer-name-{row}")).bounds().center();
    for click_count in [1usize, 2] {
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                button: gpui_kit::MouseButton::Left,
                position: at,
                click_count,
                ..Default::default()
            }),
            cx,
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                button: gpui_kit::MouseButton::Left,
                position: at,
                click_count,
                modifiers: Default::default(),
            }),
            cx,
        );
    }
    window.render_frame(cx);
}

#[gpui_kit::test]
fn rename_button_opens_a_focused_prompt_and_enter_commits(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        double_click_name(window, 0, cx);
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
        double_click_name(window, 0, cx);

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

/// The row marks its lock through the lock glyph alone, the way the reference
/// does: no badge element exists at any lock state, and the round trip still
/// works with the badge's row slot gone.
/// Ctrl-clicking the only selected member empties the selection; the row
/// must not stay active and later operations must have no target.
#[gpui_kit::test]
fn ctrl_toggle_on_the_sole_member_empties_the_selection(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let rows = super::canvas_input::layer_rows(&content, cx).0;
        assert_eq!(rows.len(), 2, "the fixture has two layers");
        let first = rows[0].id;
        click_row(window, first, gpui_kit::Modifiers::default(), cx);
        assert_eq!(
            selected_rows(&content, cx),
            vec![first],
            "the fixture starts with the top layer selected"
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
        assert!(
            selected_rows(&content, cx).is_empty(),
            "Ctrl-click on the sole member empties the selection"
        );
        content.update(cx, |app, cx| {
            assert!(
                app.selected_layer(cx).is_err(),
                "a target without a selection is refused, not resolved to another layer"
            );
        });
    })
    .expect("test window remains open");
}

/// Canvas modifier-deselect of the active member moves the active role to a
/// remaining member; a later row edit must not leave the deselected layer
/// active again, including after a projection caused by a core command.
#[gpui_kit::test]
fn canvas_deselect_of_the_active_member_moves_the_active_role(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Move, 1.0);
        window.render_frame(cx);
        let rows = super::canvas_input::layer_rows(&content, cx).0;
        let top = rows[0].id;
        let base = rows[1].id;
        click_row(window, top, gpui_kit::Modifiers::default(), cx);
        click_row(
            window,
            base,
            gpui_kit::Modifiers {
                control: true,
                ..Default::default()
            },
            cx,
        );
        assert_eq!(
            selected_rows(&content, cx),
            vec![top, base],
            "Ctrl-click adds the second layer to the selection"
        );
        let removed_core = rows
            .iter()
            .find(|row| row.id == top)
            .and_then(|row| row.core_id.clone())
            .expect("the top row carries a core id");
        content.update(cx, |app, cx| {
            assert!(
                app.move_auto_select_click(0.5, 0.5, true, cx),
                "the modifier click hit the top layer"
            );
        });
        assert_eq!(
            selected_rows(&content, cx),
            vec![base],
            "the deselected layer leaves the selection"
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).selected_layer,
            base,
            "the active role moves to the remaining member"
        );
        // A core command forces a reprojection; a deselected row must not
        // rejoin the selection because it happened to be the active row.
        content.update(cx, |app, cx| {
            app.toggle_layer_visibility(removed_core, cx);
        });
        assert_eq!(
            selected_rows(&content, cx),
            vec![base],
            "a reprojection does not resurrect the deselected row"
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).selected_layer,
            base,
            "a reprojection keeps the active role on the member"
        );
    })
    .expect("test window remains open");
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

#[gpui_kit::test]
fn right_click_on_an_unselected_row_opens_the_menu_and_selects_it(
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
        let rows = super::canvas_input::layer_rows(&content, cx).0;
        let base = rows[1].id;
        right_click_row(window, base, cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert!(state.layer_context_menu.is_some(), "the menu opened");
        assert_eq!(
            state.selected_layers,
            vec![base],
            "an unselected row becomes the selection when right-clicked"
        );
        assert!(
            window.try_find("ctx-layer-new").is_some(),
            "New Layer row mounted"
        );
        assert!(
            window.try_find("ctx-layer-delete").is_some(),
            "Delete row mounted"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn escape_closes_the_layers_row_context_menu(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        super::canvas_input::tab_to_canvas(window, cx);
        window.render_frame(cx);
        let rows = super::canvas_input::layer_rows(&content, cx).0;
        right_click_row(window, rows[1].id, cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .layer_context_menu
                .is_some()
        );
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .layer_context_menu
                .is_none(),
            "Escape closes the Layers row context menu from the canvas context"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn clicking_the_overlay_closes_the_layers_row_context_menu(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let rows = super::canvas_input::layer_rows(&content, cx).0;
        right_click_row(window, rows[1].id, cx);
        window.click("layer-context-menu-overlay", cx);
        window.render_frame(cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .layer_context_menu
                .is_none(),
            "clicking away closes the Layers row context menu"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn duplicate_from_the_row_menu_runs_the_multi_duplicate_path(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let rows = super::canvas_input::layer_rows(&content, cx).0;
        let top = rows[0].id;
        let base = rows[1].id;
        click_row(window, base, gpui_kit::Modifiers::default(), cx);
        click_row(
            window,
            top,
            gpui_kit::Modifiers {
                control: true,
                ..Default::default()
            },
            cx,
        );
        assert_eq!(selected_rows(&content, cx).len(), 2, "two layers selected");
        right_click_row(window, top, cx);
        assert_eq!(
            selected_rows(&content, cx)
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            [base, top]
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            "right-clicking a selected row keeps the multi selection"
        );
        right_click_row(window, top, cx);
        assert_eq!(
            selected_rows(&content, cx)
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            [base, top]
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            "right-clicking a selected row keeps the multi selection"
        );
        let history_before = history_cursor(&content, cx);
        window.click("ctx-layer-duplicate", cx);
        window.render_frame(cx);
        let rows_after = super::canvas_input::layer_rows(&content, cx).0;
        assert_eq!(rows_after.len(), 4, "both layers were duplicated");
        assert_eq!(
            history_cursor(&content, cx),
            history_before + 1,
            "one duplicate gesture commits one history entry"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn layers_panel_arrows_and_shortcuts_operate_the_rows(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let rows = super::canvas_input::layer_rows(&content, cx).0;
        assert_eq!(rows.len(), 2, "the fixture has two layers");
        let top = rows[0].id;
        let base = rows[1].id;

        // Focus the Layers tab button; the arrow and Enter bindings live in
        // the LayersPanel key context.
        let handle = content.read(cx).layers_tab_focus.clone();
        window.focus(&handle, cx);
        window.render_frame(cx);

        // Plain arrow: active moves one row down.
        select_layer(&content, cx, top);
        window.press("down", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).selected_layers,
            vec![base],
            "ArrowDown moves the single selection to the next row"
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).selected_layer,
            base,
            "ArrowDown moves the primary row"
        );

        // Shift+Arrow: the selection extends without dropping the prior row.
        window.press("shift-up", cx);
        window.render_frame(cx);
        assert_eq!(
            {
                let mut ids = content
                    .read(cx)
                    .state_for_test()
                    .read(cx)
                    .selected_layers
                    .clone();
                ids.sort_unstable();
                ids
            },
            vec![top, base],
            "Shift+ArrowUp extends the selection to the row above"
        );

        // Plain Enter/Shift Space: toggle visibility and lock for the tid-group.
        window.press("enter", cx);
        window.render_frame(cx);
        let toggled = flags(&content, cx);
        assert!(
            !toggled[0].visible && !toggled[1].visible,
            "Enter toggles visibility on the whole tid-group (got {toggled:?})"
        );
        window.press("space", cx);
        window.render_frame(cx);
        let toggled = flags(&content, cx);
        assert!(
            toggled[0].locked && toggled[1].locked,
            "Space toggles the base lock on the tid-group (got {toggled:?})"
        );

        // Home/End: jump the stack ends.
        window.press("end", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).selected_layer,
            base,
            "End jumps to the last row"
        );
        window.press("home", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).selected_layer,
            top,
            "Home jumps to the first row"
        );

        // Ctrl+A: select every row.
        window.press("ctrl-a", cx);
        window.render_frame(cx);
        assert_eq!(
            {
                let mut ids = content
                    .read(cx)
                    .state_for_test()
                    .read(cx)
                    .selected_layers
                    .clone();
                ids.sort_unstable();
                ids
            },
            vec![top, base],
            "Ctrl+A selects every row"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn merge_down_on_a_lone_layer_refuses_with_a_warn_toast(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            let result = app.merge_selected_layer_down(cx);
            app.report_layer_op_for_view(result, cx);
        });
        window.render_frame(cx);
        let toasts = content.read(cx).state_for_test().read(cx).toasts.clone();
        assert!(
            toasts
                .iter()
                .any(|toast| toast.message.contains("no layer below it")
                    && toast.severity == crate::state::ToastSeverity::Warn),
            "the refused merge is surfaced as a warn toast (got {toasts:?})"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn layers_and_history_tabs_switch_on_click(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("history", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layer_tab,
            LayerTab::History,
            "clicking History lights it"
        );
        window.click("layers", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layer_tab,
            LayerTab::Layers,
            "clicking Layers lights it"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn layers_and_history_tabs_cycle_from_keyboard_focus(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layer_tab,
            LayerTab::Layers,
            "the Layers tab is the resting state"
        );
        super::canvas_input::tab_to_canvas(window, cx);
        let layers_handle = content.read(cx).layers_tab_focus.clone();
        let history_handle = content.read(cx).history_tab_focus.clone();
        let mut reached = None;
        for _ in 0..32 {
            window.press("tab", cx);
            if window.focused(cx) == Some(layers_handle.clone()) {
                reached = Some("layers");
                break;
            }
            if window.focused(cx) == Some(history_handle.clone()) {
                reached = Some("history");
                break;
            }
        }
        assert_eq!(
            reached,
            Some("layers"),
            "Tab traversal reaches the Layers tab after the canvas stop"
        );
        let layers_handle = content.read(cx).layers_tab_focus.clone();
        window.focus(&layers_handle, cx);
        window.render_frame(cx);
        assert_eq!(
            window.focused(cx),
            Some(layers_handle.clone()),
            "the Layers tab accepts focus"
        );
        window.press("arrowright", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layer_tab,
            LayerTab::History,
            "ArrowRight cycles to History from Layers"
        );
        window.press("arrowright", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layer_tab,
            LayerTab::Layers,
            "ArrowRight cycles back to Layers from History"
        );
        window.press("arrowleft", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layer_tab,
            LayerTab::History,
            "ArrowLeft cycles back to History from Layers"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn apply_adjustment_bakes_the_adjustment_into_the_layer_in_one_history_entry(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with_adjusted_top(),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let rows = super::canvas_input::layer_rows(&content, cx).0;
        let top = rows
            .iter()
            .find(|row| row.core_id.as_deref() == Some("top"))
            .expect("the fixture exports a top row")
            .id;
        let before = content
            .read(cx)
            .core_layer_pixels_for_test("top")
            .expect("the top layer has a raster store")
            .2
            .clone();
        let cursor_before = history_cursor(&content, cx);
        right_click_row(window, top, cx);
        window.click("ctx-layer-apply-adjustment", cx);
        window.render_frame(cx);
        let after = content
            .read(cx)
            .core_layer_pixels_for_test("top")
            .expect("the top layer keeps its raster store")
            .2
            .clone();
        assert!(
            before
                .iter()
                .zip(after.iter())
                .any(|(before, after)| before != after),
            "a baked adjustment differs from the previewed one"
        );
        assert_eq!(
            history_cursor(&content, cx),
            cursor_before + 1,
            "one bake is one undo step"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn apply_adjustment_row_is_enabled_only_when_the_layer_has_one(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with_adjusted_top(),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let rows = super::canvas_input::layer_rows(&content, cx).0;
        let top_index = rows
            .iter()
            .position(|row| row.core_id.as_deref() == Some("top"))
            .expect("the adjusted fixture has a top row");
        let flag_before = flags(&content, cx)[top_index].has_adjustments;
        assert!(
            flag_before,
            "top starts with an active adjustment (fixture multi_layer_archive_with_adjusted_top)"
        );
        let cursor_before = history_cursor(&content, cx);
        right_click_row(window, rows[top_index].id, cx);
        window.click("ctx-layer-apply-adjustment", cx);
        window.render_frame(cx);
        let rows_after = super::canvas_input::layer_rows(&content, cx).0;
        let top_index_after = rows_after
            .iter()
            .position(|row| row.core_id.as_deref() == Some("top"))
            .expect("the top row survived the bake");
        let flag_after = flags(&content, cx)[top_index_after].has_adjustments;
        assert!(
            !flag_after,
            "the bake clears the has_adjustments flag on top"
        );

        assert_eq!(
            history_cursor(&content, cx),
            cursor_before + 1,
            "one click on Apply Adjustment commits one history entry"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn apply_adjustment_does_nothing_when_the_layer_has_no_adjustment(
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
        let rows = super::canvas_input::layer_rows(&content, cx).0;
        let top = rows[0].id;
        right_click_row(window, top, cx);
        window.click("ctx-layer-apply-adjustment", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).layers.len() == 2,
            "the disabled row did not delete any layer"
        );
    })
    .expect("test window remains open");
}

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
            "no badge element exists in the row"
        );

        window.click(format!("layer-lock-{top}"), cx);
        window.render_frame(cx);
        assert!(
            flags(&content, cx)[0].locked,
            "the row lock button locks the layer"
        );
        assert!(
            window
                .try_find(format!("layer-locked-badge-{top}"))
                .is_none(),
            "a locked row still shows no badge; the lock glyph marks it"
        );

        window.click(format!("layer-lock-{top}"), cx);
        window.render_frame(cx);
        assert!(
            !flags(&content, cx)[0].locked,
            "the second click unlocks again"
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

/// The layers empty state's rhythm matches the presets card: the supporting
/// line sits 4px under its title, not a full 12px step under it.
///
/// RED proof: against the uniform 12px-gap empty state this fails on the
/// measured 12px gap between title and supporting line.
#[gpui_kit::test]
fn layers_empty_state_keeps_the_presets_rhythm(cx: &mut gpui_kit::TestAppContext) {
    let (window, _content) = mount_app(cx, None, None);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let title = window.find("layers-empty-title").bounds();
        let sub = window.find("layers-empty-sub").bounds();
        let gap = sub.origin.y - (title.origin.y + title.size.height);
        assert!(
            (f32::from(gap) - 4.0).abs() < 0.5,
            "the supporting line follows the title by 4px, measured {gap:?}"
        );
    })
    .expect("test window remains open");
}

/// The panel's controls answer nothing while no document is open: the blend
/// select, the opacity steppers, the header lock and the footer's New Layer
/// lose their clicks, so a stray click cannot leave a command error behind.
///
/// RED proof: re-attach the handlers unconditionally in `views/layers.rs` and
/// the first click fails this on the status error it leaves behind.
#[gpui_kit::test]
fn panel_controls_answer_nothing_while_no_document_is_open(cx: &mut gpui_kit::TestAppContext) {
    let (window, content) = mount_app(cx, None, None);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("layer-blend-cycle").is_some(),
            "the panel still renders its controls over the empty state"
        );
        for id in [
            "layer-blend-cycle",
            "layer-opacity-inc",
            "layer-opacity-dec",
            "layer-lock-header-base",
            "layer-action-0",
        ] {
            window.click(id, cx);
            window.render_frame(cx);
            let state = content.read(cx).state_for_test();
            let state = state.read(cx);
            assert!(
                state.status_error.is_none(),
                "{id} must be inert without a document, not a rejected command"
            );
            assert!(
                state.layers.is_empty(),
                "{id} must not project a layer without a document"
            );
        }
    })
    .expect("test window remains open");
}
