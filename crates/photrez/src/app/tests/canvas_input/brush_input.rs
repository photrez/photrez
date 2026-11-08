//! Brush input beyond a single drag: Alt sampling, Shift axis lock, Alt right-drag on the
//! size and hardness, and the hover highlight with its latency measurement.

use super::*;

#[gpui_kit::test]
fn alt_click_with_the_brush_samples_instead_of_painting(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 1.0);
        window.render_frame(cx);
        let (w, h, before) = pixels(&content, cx);
        let history = history_version(&content, cx);
        let (bounds, placement) = page(window, (w, h));
        let start = window_point(bounds, placement, (w, h), 0.5, 0.5);
        let end = window_point(bounds, placement, (w, h), 1.5, 0.5);
        let modifiers = gpui_kit::Modifiers {
            alt: true,
            ..Default::default()
        };
        dispatch_pointer_event(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                modifiers,
                ..Default::default()
            }),
            cx,
        );
        dispatch_pointer_event(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Left),
                modifiers,
            }),
            cx,
        );
        dispatch_pointer_event(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                modifiers,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);

        assert_eq!(
            pixels(&content, cx).2,
            before,
            "Alt sampling must not paint a stroke"
        );
        assert_eq!(
            history_version(&content, cx),
            history,
            "Alt sampling must not add history"
        );
        let sampled = content
            .read(cx)
            .active_composite_pixel_for_test(1, 0)
            .expect("composite pixel under the pointer");
        let fg = content.read(cx).state_for_test().read(cx).fg_color;
        assert_eq!(fg, sampled, "the sampled colour must become the foreground");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn shift_locks_a_brush_stroke_to_the_dominant_axis(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(100, 100, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        let (width, _, before) = pixels(&content, cx);

        let (bounds, placement) = page(window, (100, 100));
        let start = window_point(bounds, placement, (100, 100), 20.0, 50.0);
        let middle = window_point(bounds, placement, (100, 100), 80.0, 58.0);
        let end = window_point(bounds, placement, (100, 100), 90.0, 60.0);
        let modifiers = gpui_kit::Modifiers {
            shift: true,
            ..Default::default()
        };
        dispatch_pointer_event(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                modifiers,
                ..Default::default()
            }),
            cx,
        );
        for position in [middle, end] {
            dispatch_pointer_event(
                window,
                PlatformInput::MouseMove(MouseMoveEvent {
                    position,
                    pressed_button: Some(MouseButton::Left),
                    modifiers,
                }),
                cx,
            );
        }
        dispatch_pointer_event(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                modifiers,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);

        let (after_width, _, after) = pixels(&content, cx);
        assert_eq!(after_width, width);
        let pixel = |rgba: &[u8], x: usize, y: usize| {
            let at = (y * width as usize + x) * 4;
            rgba[at..at + 4].to_vec()
        };
        assert_ne!(
            pixel(&after, 70, 50),
            pixel(&before, 70, 50),
            "the stroke must paint along the locked row"
        );
        assert_eq!(
            pixel(&after, 70, 58),
            pixel(&before, 70, 58),
            "the off-axis pointer path must not paint"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn eraser_on_the_background_paints_the_background_color(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(100, 100, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Eraser;
                state.eraser_size = 8.0;
                state.eraser_hardness = 1.0;
                state.eraser_opacity = 1.0;
                state.eraser_flow = 1.0;
                state.bg_color = [0, 255, 0, 255];
            });
        });
        window.render_frame(cx);
        drag_doc(window, (100, 100), (20.0, 20.0), (80.0, 20.0), cx);
        let (width, _, after) = pixels(&content, cx);
        let at = (20 * width as usize + 50) * 4;
        assert_eq!(
            &after[at..at + 4],
            &[0, 255, 0, 255],
            "background erase paints the swatch, not transparency"
        );
    })
    .expect("test window remains open");
}

/// Perf: one Alt+Right brush-adjust move updates the Size/Hardness HUD
/// and paints a frame, the surface's production update path.
#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn brush_hud_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 10.0);
        window.render_frame(cx);
        let (w, h, _before) = pixels(&content, cx);
        let (bounds, placement) = page(window, (w, h));
        let start = window_point(bounds, placement, (w, h), 0.5, 0.5);
        let modifiers = gpui_kit::Modifiers {
            alt: true,
            ..Default::default()
        };
        dispatch_pointer_event(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Right,
                position: start,
                modifiers,
                ..Default::default()
            }),
            cx,
        );
        let mut samples = Vec::new();
        for step in 1..=9 {
            let end = start + point(px(8.0 * step as f32), px(-4.0 * step as f32));
            let began = std::time::Instant::now();
            dispatch_pointer_event(
                window,
                PlatformInput::MouseMove(MouseMoveEvent {
                    position: end,
                    pressed_button: Some(MouseButton::Right),
                    modifiers,
                }),
                cx,
            );
            window.render_frame(cx);
            samples.push(began.elapsed().as_secs_f64() * 1000.0);
        }
        {
            let state = content.read(cx).state_for_test().read(cx);
            let hud = state
                .hud
                .expect("brush adjust HUD stays up during the drag");
            assert_eq!(hud.mode, crate::state::HudMode::Brush);
        }
        dispatch_pointer_event(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Right,
                position: start,
                modifiers,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "brush_hud p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "brush HUD update p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn alt_right_drag_adjusts_the_brush_size_and_hardness(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 10.0);
        content.update(cx, |app, cx| {
            app.state_for_test()
                .update(cx, |state, _| state.brush_hardness = 0.5);
        });
        window.render_frame(cx);
        let (w, h, before) = pixels(&content, cx);
        let history = history_version(&content, cx);
        let (bounds, placement) = page(window, (w, h));
        let start = window_point(bounds, placement, (w, h), 0.5, 0.5);
        let end = start + point(px(200.0), px(-125.0));
        let modifiers = gpui_kit::Modifiers {
            alt: true,
            ..Default::default()
        };
        dispatch_pointer_event(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Right,
                position: start,
                modifiers,
                ..Default::default()
            }),
            cx,
        );
        dispatch_pointer_event(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Right),
                modifiers,
            }),
            cx,
        );
        {
            let state = content.read(cx).state_for_test().read(cx);
            let hud = state.hud.expect("brush adjust HUD");
            assert_eq!(hud.mode, crate::state::HudMode::Brush);
            assert_eq!(hud.a, 20.0);
            assert!((hud.b - 100.0).abs() < 1e-6);
        }
        dispatch_pointer_event(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Right,
                position: end,
                modifiers,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);

        {
            let state = content.read(cx).state_for_test().read(cx);
            assert!(state.hud.is_none(), "release clears the brush HUD");
            assert_eq!(state.brush_size, 20.0, "200px right doubles the size");
            assert!(
                (state.brush_hardness - 1.0).abs() < 1e-6,
                "125px up adds 0.5 hardness"
            );
            assert!(
                state.context_menu.is_none(),
                "no context menu during adjust"
            );
        }
        assert_eq!(pixels(&content, cx).2, before, "adjusting must not paint");
        assert_eq!(
            history_version(&content, cx),
            history,
            "adjusting must not add history"
        );
    })
    .expect("test window remains open");
}

/// Hovering a non-active layer with Move paints the 1 px accent
/// outline; the active layer and empty canvas do
/// not.
#[gpui_kit::test]
fn hovering_a_non_active_layer_paints_the_hover_highlight(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(100, 100, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Shape, 1.0);
        drag_doc(window, (100, 100), (10.0, 10.0), (60.0, 60.0), cx);
        set_tool(&content, cx, ToolId::Move, 1.0);
        window.render_frame(cx);
        // Make the Background active so the shape is a hover candidate.
        let background = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .layers
            .iter()
            .find(|row| row.name == "Background")
            .map(|row| row.id)
            .expect("background row");
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.selected_layer = background;
                state.selected_layers = vec![background];
            });
        });
        window.render_frame(cx);

        let (bounds, placement) = page(window, (100, 100));
        let over_shape = window_point(bounds, placement, (100, 100), 30.0, 30.0);
        let _ = window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: over_shape,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        let highlight = content
            .update(cx, |app, cx| app.hover_highlight_for_test(cx))
            .expect("the hovered shape paints its outline");
        // Native shape layers are document-sized with the shape params inside
        // them, so the outline follows the layer box exactly, scaled by
        // `layer.width * scaleX`.
        assert!(
            (highlight.0 - 10.0).abs() < 1e-6
                && (highlight.1 - 10.0).abs() < 1e-6
                && (highlight.2 - 100.0).abs() < 1e-6
                && (highlight.3 - 100.0).abs() < 1e-6,
            "highlight={highlight:?}"
        );

        // Native shape layers are document-sized, so a point inside the page
        // that misses the drawn rect still hits the layer box; the no-outline
        // case is the canvas pasteboard outside the page.
        let hitbox = window.find("canvas-hitbox").bounds();
        let empty = gpui_kit::point(
            hitbox.origin.x + gpui_kit::px(5.0),
            hitbox.origin.y + gpui_kit::px(5.0),
        );
        let _ = window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: empty,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        assert!(
            content
                .update(cx, |app, cx| app.hover_highlight_for_test(cx))
                .is_none(),
            "empty canvas paints no outline"
        );
    })
    .expect("test window remains open");
}

/// The hover-move-to-frame latency for the highlight path.
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn hover_highlight_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 200, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Move, 1.0);
        window.render_frame(cx);
        // The fixture document has no second layer, so the highlight never
        // paints here; the timed path is the shared hover move-to-frame cost
        // (pointer update, hit test, overlay projection, full frame).
        let (bounds, placement) = page(window, (400, 200));
        let inside = window_point(bounds, placement, (400, 200), 200.0, 100.0);
        let outside = window_point(bounds, placement, (400, 200), 20.0, 10.0);

        // One untimed warm-up move (the dock-tab measurement protocol).
        let _ = window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: inside,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);

        let mut samples = Vec::new();
        for _sample in 0..9 {
            let start = std::time::Instant::now();
            let _ = window.dispatch_event(
                PlatformInput::MouseMove(MouseMoveEvent {
                    position: outside,
                    ..Default::default()
                }),
                cx,
            );
            window.render_frame(cx);
            let _ = window.dispatch_event(
                PlatformInput::MouseMove(MouseMoveEvent {
                    position: inside,
                    ..Default::default()
                }),
                cx,
            );
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0 / 2.0);
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "HOVER_HIGHLIGHT_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "hover highlight frame p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}
