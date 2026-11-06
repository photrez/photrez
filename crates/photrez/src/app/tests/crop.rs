use super::canvas_input::{drag_doc, page, pixels, set_tool, tab_to_canvas, window_point};
use super::*;
use crate::app::canvas_input::SetCanvasTool;
use crate::state::ToolId;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput};

fn dispatch(window: &mut gpui_kit::Window, event: PlatformInput, cx: &mut gpui_kit::App) {
    let _ = window.dispatch_event(event, cx);
    window.render_frame(cx);
}

#[gpui_kit::test]
fn crop_drag_shows_a_preview_and_escape_clears_it(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let doc = (w, h);
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 0.1, 0.1);
        let moved = window_point(bounds, placement, doc, 0.9, 0.9);
        let _ = window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                ..Default::default()
            }),
            cx,
        );
        let _ = window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        let preview = content
            .read(cx)
            .crop_rect_for_test(cx)
            .expect("the drag updates the crop preview");
        assert!(preview[2] > 0. && preview[3] > 0., "preview={preview:?}");
        assert!(
            content.read(cx).active_core_selection_for_test().is_none(),
            "a crop drag must not create a pixel selection"
        );

        tab_to_canvas(window, cx);
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).crop_rect_for_test(cx).is_none(),
            "Escape must discard the pending crop"
        );
    })
    .expect("test window remains open");
}

/// Arming Crop opens the default frame over the whole page, and
/// leaving the tool drops it.
#[gpui_kit::test]
fn selecting_crop_opens_the_frame_over_the_whole_page(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (doc_w, doc_h) = content
            .read(cx)
            .active_document_size()
            .expect("fixture document");
        assert!(
            content.read(cx).crop_rect_for_test(cx).is_none(),
            "no frame before the tool is armed"
        );
        content.update(cx, |app, cx| {
            app.set_canvas_tool_action(&SetCanvasTool(ToolId::Crop), cx);
        });
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).crop_rect_for_test(cx),
            Some([0.0, 0.0, doc_w, doc_h]),
            "arming Crop frames the whole page"
        );
        content.update(cx, |app, cx| {
            app.set_canvas_tool_action(&SetCanvasTool(ToolId::Move), cx);
        });
        window.render_frame(cx);
        assert!(
            content.read(cx).crop_rect_for_test(cx).is_none(),
            "leaving Crop drops the pending frame"
        );
    })
    .expect("test window remains open");
}

/// Perf: arming Crop seeds the full-page frame and repaints. The reset to
/// Move between samples is host-side and outside the measured span.
#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn crop_frame_open_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let arm = |content: &gpui_kit::Entity<App>, cx: &mut gpui_kit::App, tool: ToolId| {
            content.update(cx, |app, cx| {
                app.set_canvas_tool_action(&SetCanvasTool(tool), cx);
            });
        };
        arm(&content, cx, ToolId::Move);
        window.render_frame(cx);
        arm(&content, cx, ToolId::Crop);
        window.render_frame(cx);
        let mut samples = Vec::new();
        for _ in 0..9 {
            arm(&content, cx, ToolId::Move);
            window.render_frame(cx);
            let start = std::time::Instant::now();
            arm(&content, cx, ToolId::Crop);
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "crop_frame_open p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "crop frame open p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn enter_applies_the_crop_and_undo_restores_the_document_size(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((2, 1))
        );
        drag_doc(window, (w, h), (0.1, 0.1), (0.9, 0.9), cx);
        assert!(
            content.read(cx).crop_rect_for_test(cx).is_some(),
            "the drag sets the crop frame"
        );

        tab_to_canvas(window, cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((1, 1)),
            "Enter must apply the crop"
        );
        assert!(content.read(cx).crop_rect_for_test(cx).is_none());
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_tool,
            ToolId::Move,
            "applying a crop returns to Move"
        );
        assert!(
            content.read(cx).active_core_selection_for_test().is_none(),
            "the crop clears the pixel selection"
        );

        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((2, 1)),
            "undo restores the pre-crop document size"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn arrow_keys_nudge_the_crop_frame_within_the_document(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let doc = (w, h);
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 0.1, 0.1);
        let end = window_point(bounds, placement, doc, 0.9, 0.9);
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                ..Default::default()
            }),
            cx,
        );
        let before = content
            .read(cx)
            .crop_rect_for_test(cx)
            .expect("the crop frame exists");
        tab_to_canvas(window, cx);
        window.press("right", cx);
        window.render_frame(cx);
        let nudged = content
            .read(cx)
            .crop_rect_for_test(cx)
            .expect("the crop frame survives the nudge");
        assert_eq!(nudged[0], (before[0] + 1.0).min(doc.0 as f64 - before[2]));
        assert_eq!(nudged[1], before[1]);
        assert_eq!(nudged[2], before[2]);
        assert_eq!(nudged[3], before[3]);
        // The 1-pixel-wide document clamps a left nudge at zero.
        window.press("left", cx);
        window.press("left", cx);
        window.render_frame(cx);
        assert_eq!(
            content
                .read(cx)
                .crop_rect_for_test(cx)
                .expect("frame exists")[0],
            0.0
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_without_a_frame_is_refused_without_history(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        let cursor_before = content
            .read(cx)
            .active_core_history_cursor_for_test()
            .unwrap();
        content.update(cx, |app, cx| {
            let error = app.apply_crop(cx).expect_err("no frame is refused");
            assert!(error.contains("no crop rectangle"), "{error}");
        });
        assert_eq!(
            content
                .read(cx)
                .active_core_history_cursor_for_test()
                .unwrap(),
            cursor_before
        );
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((2, 1))
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_bar_apply_and_cancel_are_wired(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        assert!(
            window.try_find("option-live-crop-apply").is_none(),
            "the Crop live strip is retired"
        );
        let (w, h, _) = pixels(&content, cx);

        drag_doc(window, (w, h), (0.1, 0.1), (0.9, 0.9), cx);
        assert!(content.read(cx).crop_rect_for_test(cx).is_some());
        window.click("opt-cancel", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).crop_rect_for_test(cx).is_none(),
            "Cancel clears the frame"
        );

        drag_doc(window, (w, h), (0.1, 0.1), (0.9, 0.9), cx);
        let before = content
            .read(cx)
            .active_document_dims_for_test()
            .expect("document dims");
        window.click("opt-apply", cx);
        window.render_frame(cx);
        assert!(content.read(cx).crop_rect_for_test(cx).is_none());
        let after = content
            .read(cx)
            .active_document_dims_for_test()
            .expect("document dims");
        assert!(
            after.0 <= before.0 && after.1 <= before.1 && after != before,
            "Apply committed the crop: {before:?} -> {after:?}"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn ctrl_z_undoes_crop_frame_steps_before_document_history(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.1, 0.1), (0.9, 0.9), cx);
        let frame = content.read(cx).crop_rect_for_test(cx).expect("frame");
        tab_to_canvas(window, cx);
        window.press("right", cx);
        window.render_frame(cx);
        let nudged = content.read(cx).crop_rect_for_test(cx).expect("frame");
        assert_ne!(nudged[0], frame[0]);
        let history_before = content
            .read(cx)
            .active_core_history_cursor_for_test()
            .unwrap();

        window.press("ctrl-z", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).crop_rect_for_test(cx).expect("frame")[0],
            frame[0],
            "Ctrl+Z restores the previous crop frame"
        );
        assert_eq!(
            content
                .read(cx)
                .active_core_history_cursor_for_test()
                .unwrap(),
            history_before,
            "crop undo does not step document history"
        );

        window.press("ctrl-shift-z", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).crop_rect_for_test(cx).expect("frame")[0],
            nudged[0],
            "Ctrl+Shift+Z redoes the crop step"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_frame_moves_from_inside_and_resizes_from_a_corner(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test()
                .update(cx, |state, _| state.active_tool = ToolId::Crop);
        });
        window.render_frame(cx);
        drag_doc(window, (400, 300), (50.0, 50.0), (250.0, 200.0), cx);
        let frame = content.read(cx).crop_rect_for_test(cx).expect("frame");
        assert!((frame[2] - 200.0).abs() < 3.0, "frame={frame:?}");

        // Drag from inside the frame: move, size unchanged.
        drag_doc(window, (400, 300), (150.0, 120.0), (200.0, 150.0), cx);
        let moved = content.read(cx).crop_rect_for_test(cx).expect("frame");
        assert!(
            (moved[0] - (frame[0] + 50.0)).abs() < 3.0,
            "moved={moved:?}"
        );
        assert!((moved[1] - (frame[1] + 30.0)).abs() < 3.0);
        assert!((moved[2] - frame[2]).abs() < 1.0);
        assert!((moved[3] - frame[3]).abs() < 1.0);

        // Drag the top-left corner outward: resize.
        let (left, top, width, height) = (moved[0], moved[1], moved[2], moved[3]);
        drag_doc(
            window,
            (400, 300),
            (left as f32, top as f32),
            ((left + 20.0) as f32, (top + 10.0) as f32),
            cx,
        );
        let resized = content.read(cx).crop_rect_for_test(cx).expect("frame");
        assert!(
            resized[2] < width - 10.0 && resized[3] < height - 5.0,
            "resized={resized:?} from {moved:?}"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_bar_checkboxes_toggle_real_state(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test();
        assert!(state.read(cx).crop_delete_pixels, "Delete ships checked");
        assert!(state.read(cx).crop_fill_background, "Fill BG ships checked");

        window.click("opt-crop-delete", cx);
        window.render_frame(cx);
        assert!(
            !content
                .read(cx)
                .state_for_test()
                .read(cx)
                .crop_delete_pixels,
            "clicking Delete clears the flag"
        );
        window.click("opt-crop-fill", cx);
        window.render_frame(cx);
        assert!(
            !content
                .read(cx)
                .state_for_test()
                .read(cx)
                .crop_fill_background,
            "clicking Fill BG clears the flag"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn apply_crop_passes_the_checkbox_flags_to_core(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);

        // Defaults (both checked): destructive crop plus a fill in the
        // Background colour.
        content.update(cx, |app, cx| {
            app.state
                .update(cx, |state, _| state.crop_rect = Some([0.0, 0.0, 2.0, 1.0]));
            app.apply_crop(cx).expect("crop applies");
        });
        let bg = content.read(cx).state_for_test().read(cx).bg_color;
        assert_eq!(
            content.read(cx).state_for_test().read(cx).last_crop_flags,
            Some((true, Some(bg))),
            "checked boxes send delete + fill"
        );

        // Unchecking both must reach core as (false, None). Apply returns to
        // Move, so re-select Crop first.
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            app.state.update(cx, |state, _| {
                state.crop_delete_pixels = false;
                state.crop_fill_background = false;
                state.crop_rect = Some([0.0, 0.0, 2.0, 1.0]);
            });
            app.apply_crop(cx).expect("crop applies");
        });
        assert_eq!(
            content.read(cx).state_for_test().read(cx).last_crop_flags,
            Some((false, None)),
            "unchecked boxes send non-destructive, no fill"
        );
    })
    .expect("test window remains open");
}

/// Crop bar `Delete` toggle latency: raw MouseDown+MouseUp at the control plus
/// one `render_frame`. The kit `click` helper must not be timed (it renders
/// before dispatching). `#[ignore]`: perf probes run explicitly.
#[gpui_kit::test]
#[ignore]
fn crop_toggle_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        let press = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
            let position = window.find("opt-crop-delete").bounds().center();
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
        };
        // One unmeasured warm-up.
        press(window, cx);
        window.render_frame(cx);

        let mut samples = Vec::new();
        for sample in 0..9 {
            let expected = !content
                .read(cx)
                .state_for_test()
                .read(cx)
                .crop_delete_pixels;
            let start = std::time::Instant::now();
            press(window, cx);
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(
                content
                    .read(cx)
                    .state_for_test()
                    .read(cx)
                    .crop_delete_pixels,
                expected,
                "sample {sample} toggles the flag"
            );
        }
        let mut sorted = samples.clone();
        sorted.sort_by(f64::total_cmp);
        let p50 = sorted[sorted.len() / 2];
        let p95 = sorted[(sorted.len() as f64 * 0.95).ceil() as usize - 1];
        eprintln!("CROP_TOGGLE_MS samples={samples:?} p50={p50:.3} p95={p95:.3}");
        assert!(
            p95 <= 33.3,
            "crop toggle p95 {p95:.3} ms exceeds the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

/// The ratio dropdown is live. Choosing 1:1 (inside the list's 300 px
/// viewport) refits the pending frame to the largest square inside the
/// document and records the recent.
#[gpui_kit::test]
fn crop_ratio_dropdown_refits_the_pending_frame(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        window.click("crop-ratio", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).crop_ratio_open,
            "the ratio list expands"
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_mode,
            crate::crop_aspect::CropMode::Free
        );
        let _active_row = window.find("crop-ratio-free");
        window.click("crop-ratio-preset-0", cx);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(
            state.crop_mode,
            crate::crop_aspect::CropMode::Ratio,
            "picking a preset switches to Ratio"
        );
        assert_eq!(state.crop_aspect, Some((1, 1)));
        assert_eq!(state.recent_crop_ratios, vec![(1, 1)]);
        let rect = state.crop_rect.expect("the preset refits the frame");
        let expected =
            crate::crop_aspect::fit_crop_rect_to_aspect((1, 1), f64::from(w), f64::from(h), 0.0);
        assert_eq!(rect, expected, "1:1 fits the document height");
    })
    .expect("test window remains open");
}

/// `Lock Current Shape` is inert outside Free: the control is disabled unless
/// the mode is Free; the Size mode keeps its mode.
#[gpui_kit::test]
fn crop_lock_shape_is_inert_outside_free_mode(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        window.click("crop-ratio", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        window.click("crop-ratio-size", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_mode,
            crate::crop_aspect::CropMode::Size,
            "the Size row switches the mode"
        );
        window.click("crop-ratio", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        window.click("crop-ratio-lock", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(
            state.crop_mode,
            crate::crop_aspect::CropMode::Size,
            "a disabled Lock Current Shape row must not switch the mode"
        );
        assert!(
            state.crop_ratio_open,
            "an inert row must not dismiss the list either"
        );
    })
    .expect("test window remains open");
}

/// The Size mode converts entry at the crop bar's 300 PPI base,
/// keeps the pixel target authoritative across unit changes and swap.
#[gpui_kit::test]
fn crop_size_mode_converts_at_300_ppi_and_swaps(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        window.click("crop-ratio", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).crop_ratio_open,
            "the ratio list opens before choosing Size"
        );
        window.click("crop-ratio-size", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(
                state.crop_mode,
                crate::crop_aspect::CropMode::Size,
                "the Size row switches the mode; open={}",
                state.crop_ratio_open
            );
            assert_eq!(state.crop_size_target, Some((800, 600)));
            assert_eq!(
                (state.crop_size_w.as_str(), state.crop_size_h.as_str()),
                ("800", "600")
            );
        }

        // Switch the unit to inches: the pixel target stays 800x600.
        window.click("crop-size-unit", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        window.click("crop-size-unit-in", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.crop_size_unit, crate::crop_aspect::CropSizeUnit::In);
            assert_eq!(state.crop_size_target, Some((800, 600)));
            assert_eq!(
                (state.crop_size_w.as_str(), state.crop_size_h.as_str()),
                ("2.67", "2.00"),
                "800 px is 2.67 in at 300 PPI"
            );
        }

        // Type 4 into W: 4 in * 300 = 1200 px.
        window.click("crop-size-w", cx);
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        // The field selects its value on focus, so the first digit replaces it.
        window.press("4", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.crop_size_target, Some((1200, 600)), "4 in at 300 PPI");
            assert_eq!(state.crop_size_w, "4");
        }

        // Swap: 1200x600 becomes 600x1200, and the buffers swap with it.
        window.click("crop-size-swap", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(state.crop_size_target, Some((600, 1200)));
        assert_eq!(
            (state.crop_size_w.as_str(), state.crop_size_h.as_str()),
            ("2.00", "4")
        );
    })
    .expect("test window remains open");
}

/// Perf: choosing the Size mode (`crop-ratio` -> `crop-ratio-size`)
/// repaints the bar with the W/H fields and the unit trigger.
#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn crop_size_mode_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        let reopen = |content: &gpui_kit::Entity<App>, cx: &mut gpui_kit::App| {
            content.update(cx, |app, cx| {
                app.state_for_test().update(cx, |state, _| {
                    state.crop_ratio_open = true;
                    state.crop_mode = crate::crop_aspect::CropMode::Free;
                });
            });
        };
        let press = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
            let position = window.find("crop-ratio-size").bounds().center();
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
        };
        reopen(&content, cx);
        window.render_frame(cx);
        window.render_frame(cx);
        press(window, cx);
        window.render_frame(cx);
        let mut samples = Vec::new();
        for _ in 0..9 {
            reopen(&content, cx);
            window.render_frame(cx);
            window.render_frame(cx);
            let start = std::time::Instant::now();
            press(window, cx);
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "crop_size_mode p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "crop size mode p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

/// Perf: opening/closing the Aspect Ratio dropdown rebuilds the list and
/// repaints the option bar.
#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn crop_ratio_open_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        // The press helper renders around the dispatch, so the replayed pair of
        // events plus one frame is the event-to-frame metric.
        let press = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
            let position = window.find("crop-ratio").bounds().center();
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
        };
        // One unmeasured warm-up.
        press(window, cx);
        window.render_frame(cx);
        let mut samples = Vec::new();
        for _ in 0..9 {
            let start = std::time::Instant::now();
            press(window, cx);
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "crop_ratio_open p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "crop ratio open p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

/// A saved crop-bar capture at 1536 layout px paints two narrow field-filled boxes
/// near the left that this build's resting capture does not show, which could mean a
/// control was lost at that width. Enumerate the bar's controls at that width
/// instead of reading the answer back off pixels.
#[gpui_kit::test]
fn crop_bar_keeps_every_control_at_the_capture_width(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1536.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);

        let ids = [
            "crop-ratio",
            "opt-crop-delete",
            "opt-crop-fill",
            "opt-cancel",
            "opt-apply",
        ];
        let missing: Vec<&str> = ids
            .iter()
            .copied()
            .filter(|id| window.try_find(*id).is_none())
            .collect();
        assert!(
            missing.is_empty(),
            "the crop bar dropped controls at the capture width: {missing:?}"
        );
        for id in ids {
            let bounds = window.find(id).bounds();
            assert!(
                f32::from(bounds.size.width) > 0.0 && f32::from(bounds.size.height) > 0.0,
                "{id} is laid out with no area at the capture width"
            );
        }
    })
    .expect("test window remains open");
}

/// The crop bar ships no More trigger. The bar contributes no overflow group,
/// so a mounted trigger would paint and open nothing.
///
/// `more_dropdown_button` carries no `test_support()`, so `try_find` cannot see
/// it; the observation is geometric instead. Measured at 1536 px: with the
/// trigger the span from the fill checkbox to Cancel is 132 px (swatch group,
/// two dividers, the 24 px button and their gaps); the divider-only layout
/// measures 97 px.
///
/// RED proof: restore `.child(more_dropdown_button())` plus one divider in
/// `variants::crop_items` and the first assertion fails on the measured span.
#[gpui_kit::test]
fn crop_bar_mounts_no_more_trigger(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1536.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        let fill = window.find("opt-crop-fill").bounds();
        let cancel = window.find("opt-cancel").bounds();
        let span = f32::from(cancel.origin.x - (fill.origin.x + fill.size.width));
        assert!(
            span < 115.0,
            "the crop bar mounts a dead More trigger: the fill-to-Cancel span is \
             {span:.1} px, the divider-only layout measures 97 px"
        );
        for id in [
            "crop-ratio",
            "opt-crop-delete",
            "opt-crop-fill",
            "opt-cancel",
            "opt-apply",
        ] {
            assert!(
                window.try_find(id).is_some(),
                "{id} still mounts after the trigger removal"
            );
        }
    })
    .expect("test window remains open");
}
