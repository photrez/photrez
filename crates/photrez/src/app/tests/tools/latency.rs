//! Response-time measurements for the tool interactions, kept together because they
//! share the same timing harness.

use super::*;

/// The Move bar's toggle response (dispatch + one render).
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn move_bar_toggle_click_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, _content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let mut samples = Vec::new();
        for _ in 0..9 {
            let position = window.find("opt-snap").bounds().center();
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
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S030_MOVE_BAR_TOGGLE_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "Move bar toggle p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}

/// The inspector dock tab switch (dispatch + one render).
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn dock_tab_switch_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        // Warm the panel body once; the first switch pays the mount cost.
        window.click("adjustments", cx);
        window.render_frame(cx);
        let mut samples = Vec::new();
        let mut next = "properties";
        for _ in 0..9 {
            let position = window.find(next).bounds().center();
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
            let tab = content.read(cx).state_for_test().read(cx).inspector_tab;
            if next == "adjustments" {
                assert_eq!(tab, crate::state::InspectorTab::Adjustments);
                next = "properties";
            } else {
                assert_eq!(tab, crate::state::InspectorTab::Properties);
                next = "adjustments";
            }
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S050_DOCK_TAB_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "dock tab switch p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}

/// Opening the brush preset menu from the overflow (dispatch + render).
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn brush_overflow_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 20.0);
        window.render_frame(cx);
        let press = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App, id: &'static str| {
            let position = window.find(id).bounds().center();
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
        // Warm once on the overflow toggle, then leave it closed.
        press(window, cx, "opt-more");
        window.render_frame(cx);
        assert!(window.try_find("opt-more-panel").is_some());
        press(window, cx, "opt-more");
        window.render_frame(cx);

        let mut samples = Vec::new();
        for sample in 0..9 {
            let start = std::time::Instant::now();
            press(window, cx, "opt-more");
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert!(
                window.try_find("opt-more-panel").is_some(),
                "sample {sample} shows the overflow panel"
            );
            press(window, cx, "opt-more");
            window.render_frame(cx);
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S031_BRUSH_OVERFLOW_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "brush overflow p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}

/// Opening the shape kind menu (dispatch + render).
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn shape_kind_menu_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);
        let press = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App, id: &'static str| {
            let position = window.find(id).bounds().center();
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
        // Warm once, then close the menu again.
        press(window, cx, "opt-shape-kind");
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).open_select,
            Some("opt-shape-kind")
        );
        window.click("canvas-hitbox", cx);
        window.render_frame(cx);

        let mut samples = Vec::new();
        for sample in 0..9 {
            let start = std::time::Instant::now();
            press(window, cx, "opt-shape-kind");
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(
                content.read(cx).state_for_test().read(cx).open_select,
                Some("opt-shape-kind"),
                "sample {sample} opens the kind menu"
            );
            window.click("canvas-hitbox", cx);
            window.render_frame(cx);
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S038_SHAPE_KIND_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "shape kind menu p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}

/// The shape tool's release -> committed rectangle frame. The drag's
/// move frames are warm-up: the measured event is the one that commits the
/// layer and paints the result, like the other tool-commit budgets.
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn shape_draw_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let mut samples = Vec::new();
        for _ in 0..9 {
            content.update(cx, |app, cx| {
                app.create_document(800, 600, cx).expect("blank document");
            });
            set_tool(&content, cx, ToolId::Shape, 1.0);
            window.render_frame(cx);
            let (bounds, page_bounds) = page(window, (800, 600));
            let from = window_point(bounds, page_bounds, (800, 600), 10.0, 10.0);
            let to = window_point(bounds, page_bounds, (800, 600), 100.0, 100.0);
            let _ = window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position: from,
                    ..Default::default()
                }),
                cx,
            );
            for step in 1..=8 {
                let fraction = step as f32 / 8.;
                let position = gpui_kit::point(
                    from.x + (to.x - from.x) * fraction,
                    from.y + (to.y - from.y) * fraction,
                );
                let _ = window.dispatch_event(
                    PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                        position,
                        pressed_button: Some(MouseButton::Left),
                        modifiers: Default::default(),
                    }),
                    cx,
                );
            }
            let start = std::time::Instant::now();
            let _ = window.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position: to,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(
                content.read(cx).active_core_layer_count_for_test(),
                Some(2),
                "the release commits one Shape layer over the Background"
            );
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "shape_draw p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "shape draw p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

/// One pointer move over the page with the brush armed, from the event
/// dispatch through the frame that paints the cursor ring. Measured on a
/// fresh 800x600 document per sample, like the other overlay latencies.
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn brush_cursor_hover_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let mut samples = Vec::new();
        for _ in 0..9 {
            content.update(cx, |app, cx| {
                app.create_document(800, 600, cx).expect("blank document");
            });
            set_tool(&content, cx, ToolId::Brush, 1.0);
            window.render_frame(cx);
            let (bounds, page_bounds) = page(window, (800, 600));
            let position = window_point(bounds, page_bounds, (800, 600), 400.0, 300.0);
            let start = std::time::Instant::now();
            let _ = window.dispatch_event(
                PlatformInput::MouseMove(MouseMoveEvent {
                    position,
                    ..Default::default()
                }),
                cx,
            );
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert!(
                content
                    .update(cx, |app, cx| app.brush_cursor_for_test(cx))
                    .is_some(),
                "the hover frame paints the ring"
            );
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "brush_cursor_hover p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "brush cursor hover p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

/// The gradient tool's release -> committed gradient frame, measured on
/// a fresh 800x600 document like the shape commit above.
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn gradient_draw_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let mut samples = Vec::new();
        for _ in 0..9 {
            content.update(cx, |app, cx| {
                app.create_document(800, 600, cx).expect("blank document");
            });
            set_tool(&content, cx, ToolId::Gradient, 1.0);
            window.render_frame(cx);
            let before = content
                .read(cx)
                .active_core_history_version_for_test()
                .expect("history version");
            let (bounds, page_bounds) = page(window, (800, 600));
            let from = window_point(bounds, page_bounds, (800, 600), 10.0, 10.0);
            let to = window_point(bounds, page_bounds, (800, 600), 100.0, 100.0);
            let _ = window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position: from,
                    ..Default::default()
                }),
                cx,
            );
            for step in 1..=8 {
                let fraction = step as f32 / 8.;
                let position = gpui_kit::point(
                    from.x + (to.x - from.x) * fraction,
                    from.y + (to.y - from.y) * fraction,
                );
                let _ = window.dispatch_event(
                    PlatformInput::MouseMove(MouseMoveEvent {
                        position,
                        pressed_button: Some(MouseButton::Left),
                        modifiers: Default::default(),
                    }),
                    cx,
                );
            }
            let start = std::time::Instant::now();
            let _ = window.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position: to,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert_ne!(
                content
                    .read(cx)
                    .active_core_history_version_for_test()
                    .expect("history version"),
                before,
                "the release commits the gradient through core history"
            );
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "gradient_draw p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "gradient draw p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

/// Picking a bucket tolerance preset (dispatch + render).
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn bucket_tolerance_preset_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::PaintBucket, 1.0);
        window.render_frame(cx);
        let press = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App, id: &'static str| {
            let position = window.find(id).bounds().center();
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
        // Warm once, then return to the 32 preset.
        press(window, cx, "opt-tol-64");
        window.render_frame(cx);
        press(window, cx, "opt-tol-32");
        window.render_frame(cx);

        let mut samples = Vec::new();
        for sample in 0..9 {
            let start = std::time::Instant::now();
            press(window, cx, "opt-tol-64");
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert!(
                content.read(cx).state_for_test().read(cx).fill_tolerance == 64,
                "sample {sample} selects the 64 preset"
            );
            press(window, cx, "opt-tol-32");
            window.render_frame(cx);
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S032_BUCKET_TOL_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "bucket tolerance preset p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}

/// Hovering the eyedropper HEX badge (hover -> tooltip paint).
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn eyedropper_hex_tooltip_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Eyedropper, 1.0);
        window.render_frame(cx);
        // Warm once, then leave the pointer off the badge.
        window.hover("opt-copy-hex", cx);
        window.render_frame(cx);
        assert!(window.try_find("opt-copy-hex-tooltip").is_some());
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);

        let mut samples = Vec::new();
        for sample in 0..9 {
            let start = std::time::Instant::now();
            window.hover("opt-copy-hex", cx);
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert!(
                window.try_find("opt-copy-hex-tooltip").is_some(),
                "sample {sample} shows the copy tooltip"
            );
            window.hover("canvas-hitbox", cx);
            window.render_frame(cx);
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S034_HEX_TOOLTIP_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "eyedropper hex tooltip p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}

/// Opening the gradient type menu (dispatch + render).
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn gradient_type_menu_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Gradient, 1.0);
        window.render_frame(cx);
        let press = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App, id: &'static str| {
            let position = window.find(id).bounds().center();
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
        // Warm once, then close the menu again.
        press(window, cx, "opt-gradient-type");
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).open_select,
            Some("opt-gradient-type")
        );
        window.click("canvas-hitbox", cx);
        window.render_frame(cx);

        let mut samples = Vec::new();
        for sample in 0..9 {
            let start = std::time::Instant::now();
            press(window, cx, "opt-gradient-type");
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(
                content.read(cx).state_for_test().read(cx).open_select,
                Some("opt-gradient-type"),
                "sample {sample} opens the type menu"
            );
            window.click("canvas-hitbox", cx);
            window.render_frame(cx);
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S035_GRAD_TYPE_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "gradient type menu p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}
