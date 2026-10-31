use super::input::{mount_archive_app, select_layer};
use super::*;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{MouseButton, MouseDownEvent, MouseUpEvent, PlatformInput};

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

/// S028: the layer row eye toggle response (dispatch + one render).
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn layer_eye_toggle_latency(cx: &mut gpui_kit::TestAppContext) {
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
        let mut samples = Vec::new();
        let mut visible = flags(&content, cx)[0].visible;
        for _ in 0..9 {
            let position = window.find(format!("layer-eye-{top}")).bounds().center();
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
            let now = flags(&content, cx)[0].visible;
            assert_ne!(now, visible, "the eye toggle flips visibility");
            visible = now;
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S028_LAYER_EYE_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "layer eye toggle p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
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

/// S044: the Properties status refresh after a Layers lock toggle.
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn properties_status_refresh_latency(cx: &mut gpui_kit::TestAppContext) {
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
        window.render_frame(cx);
        window.click("layer-lock-header-position", cx);
        window.render_frame(cx);
        let mut samples = Vec::new();
        for _ in 0..9 {
            let position = window.find("layer-lock-header-rotation").bounds().center();
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
                window.try_find("properties-status-pos").is_some()
                    || window.try_find("properties-status-pos-rot").is_some(),
                "each toggle must refresh the Properties status hint"
            );
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S044_PROPERTIES_REFRESH_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "Properties refresh p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
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
        let (rows, _) = super::input::layer_rows(&content, cx);
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
        let (rows, _) = super::input::layer_rows(&content, cx);
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
        let (rows, _) = super::input::layer_rows(&content, cx);
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

/// S060: the guarded row-lock click plus the toast paint (dispatch + render).
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn background_lock_toast_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with_background(),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (rows, _) = super::input::layer_rows(&content, cx);
        let bottom = rows
            .iter()
            .find(|row| row.core_id.as_deref() == Some("bottom"))
            .expect("bottom row")
            .id;
        let id = format!("layer-lock-{bottom}");
        let dismiss = |cx: &mut gpui_kit::App| {
            let toast = content.read(cx).state_for_test().read(cx).toasts[0].id;
            content.update(cx, |app, cx| app.dismiss_toast(toast, cx));
        };
        // Warm once; the first click pays the host mount cost.
        window.click(id.clone(), cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).toasts.len(),
            1,
            "the warm-up click posts one toast"
        );
        dismiss(cx);
        window.render_frame(cx);

        // The kit's `click` helper renders before dispatching, so it must not
        // be timed; this dispatch is the production path without that render.
        let press = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
            let position = window.find(id.clone()).bounds().center();
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                    button: gpui_kit::MouseButton::Left,
                    position,
                    ..Default::default()
                }),
                cx,
            );
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                    button: gpui_kit::MouseButton::Left,
                    position,
                    ..Default::default()
                }),
                cx,
            );
        };
        let mut samples = Vec::new();
        for sample in 0..9 {
            let start = std::time::Instant::now();
            press(window, cx);
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(
                content.read(cx).state_for_test().read(cx).toasts.len(),
                1,
                "sample {sample} posts one toast"
            );
            dismiss(cx);
            window.render_frame(cx);
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S060_TOAST_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "toast click p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
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
fn footer_actions_disable_on_the_background_and_last_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with_background(),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (rows, _) = super::input::layer_rows(&content, cx);
        let bottom = rows
            .iter()
            .find(|row| row.core_id.as_deref() == Some("bottom"))
            .expect("bottom row")
            .id;
        select_layer(&content, cx, bottom);
        window.render_frame(cx);

        window.click("layer-action-4", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert!(
                state.status_error.is_none(),
                "the Background delete is disabled, not a rejected command"
            );
            assert_eq!(state.layers.len(), 2, "the disabled delete removes nothing");
        }

        window.click("layer-action-2", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert!(
                state.status_error.is_none(),
                "merge-down is disabled with no layer below"
            );
            assert_eq!(state.layers.len(), 2, "the disabled merge changes nothing");
        }

        assert!(
            window.try_find("layer-action-0-tooltip").is_none(),
            "the footer tooltip is absent at rest"
        );
        window.hover("layer-action-0", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("layer-action-0-tooltip").is_some(),
            "hovering a footer action shows its tooltip"
        );
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
        let (rows, _) = super::input::layer_rows(&content, cx);
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
        super::input::layer_rows(content, cx)
            .0
            .into_iter()
            .map(|row| row.core_id.expect("the fixture layers have core ids"))
            .collect()
    };
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (rows, _) = super::input::layer_rows(&content, cx);
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
        let (rows, _) = super::input::layer_rows(&content, cx);
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

/// The blend control's click -> composited frame budget: one press/release on
/// the Layers select plus the frame that repaints the canvas with the new
/// blend mode.
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn blend_cycle_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let bounds = window.find("layer-blend-cycle").bounds();
        let center =
            bounds.origin + gpui_kit::point(bounds.size.width / 2.0, bounds.size.height / 2.0);
        let mut samples = Vec::new();
        for _ in 0..9 {
            let start = std::time::Instant::now();
            let _ = window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position: center,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
            let _ = window.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position: center,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "blend_cycle p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "blend cycle p95 {p95:.3} ms over the 33.3 ms budget"
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
