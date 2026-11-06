use super::canvas_input::{set_tool, tab_to_canvas};
use super::*;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{MouseButton, MouseDownEvent, MouseUpEvent, PlatformInput};

fn right_click(
    window: &mut gpui_kit::Window,
    position: gpui_kit::Point<gpui_kit::Pixels>,
    cx: &mut gpui_kit::App,
) {
    let _ = window.dispatch_event(
        PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Right,
            position,
            ..Default::default()
        }),
        cx,
    );
    let _ = window.dispatch_event(
        PlatformInput::MouseUp(MouseUpEvent {
            button: MouseButton::Right,
            position,
            ..Default::default()
        }),
        cx,
    );
}

fn click_track(
    window: &mut gpui_kit::Window,
    surface: gpui_kit::Bounds<gpui_kit::Pixels>,
    percent: f32,
    cx: &mut gpui_kit::App,
) {
    window.release_pointer();
    let position = gpui_kit::point(
        surface.origin.x + surface.size.width * percent,
        surface.origin.y + surface.size.height / 2.0,
    );
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
    window.release_pointer();
}

fn surface_bounds(
    content: &Entity<App>,
    cx: &gpui_kit::App,
    pick: impl Fn(&crate::state::AppState) -> Option<gpui_kit::Bounds<gpui_kit::Pixels>>,
) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    let state = content.read(cx).state_for_test().read(cx);
    pick(state).expect("the slider track is painted")
}

#[gpui_kit::test]
fn right_click_with_the_brush_opens_the_live_brush_menu(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 20.0);
        window.render_frame(cx);
        let position = window.find("canvas-hitbox").bounds().center();
        right_click(window, position, cx);
        window.render_frame(cx);

        let anchor = content
            .read(cx)
            .brush_menu_for_test(cx)
            .expect("the brush right-click opens its panel");
        assert!((anchor.0 - f32::from(position.x)).abs() < 1.0);
        assert!((anchor.1 - f32::from(position.y)).abs() < 1.0);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .context_menu
                .is_none(),
            "the canvas menu does not open for the brush"
        );
        let _ = window.find("brush-menu");
    })
    .expect("test window remains open");
}

/// Right-click -> panel paint. Ignored by default because parallel test load
/// inflates the percentiles; run with `--ignored --exact`.
#[gpui_kit::test]
#[ignore = "perf measurement; run serially with --ignored --exact"]
fn brush_menu_open_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 20.0);
        window.render_frame(cx);
        let position = window.find("canvas-hitbox").bounds().center();
        // One unmeasured warm-up open/close cycle.
        right_click(window, position, cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| app.close_brush_menu(cx));
        window.render_frame(cx);

        let mut samples = Vec::new();
        for sample in 0..9 {
            let start = std::time::Instant::now();
            right_click(window, position, cx);
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert!(
                content.read(cx).brush_menu_for_test(cx).is_some(),
                "sample {sample} opens the panel"
            );
            let _ = window.find("brush-menu");
            content.update(cx, |app, cx| app.close_brush_menu(cx));
            window.render_frame(cx);
        }
        let mut sorted = samples.clone();
        sorted.sort_by(f64::total_cmp);
        let p50 = sorted[sorted.len() / 2];
        let p95 = sorted[(sorted.len() as f64 * 0.95).ceil() as usize - 1];
        eprintln!("BRUSH_MENU_MS samples={samples:?} p50={p50:.3} p95={p95:.3}");
        assert!(
            p95 <= 33.3,
            "brush menu open p95 {p95:.3} ms exceeds the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn right_click_with_the_move_tool_keeps_the_canvas_menu(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Move, 20.0);
        window.render_frame(cx);
        let position = window.find("canvas-hitbox").bounds().center();
        right_click(window, position, cx);
        window.render_frame(cx);

        assert!(content.read(cx).brush_menu_for_test(cx).is_none());
        assert!(content
            .read(cx)
            .state_for_test()
            .read(cx)
            .context_menu
            .is_some());
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn brush_menu_sliders_drive_size_hardness_and_strength(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 20.0);
        content.update(cx, |app, cx| {
            app.state_for_test()
                .update(cx, |state, _| state.brush_preset_index = 0);
        });
        window.render_frame(cx);
        let position = window.find("canvas-hitbox").bounds().center();
        right_click(window, position, cx);
        window.render_frame(cx);

        // 50 % of the track: round(1 + 499 * 50/75) = 334 px.
        let size = surface_bounds(&content, cx, |state| state.brush_menu_size_bounds.get());
        click_track(window, size, 0.5, cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.brush_size, 334.0);
            assert_eq!(
                state.brush_preset_index,
                usize::MAX,
                "a slider edit clears the preset"
            );
        }

        let hardness = surface_bounds(&content, cx, |state| state.brush_menu_hardness_bounds.get());
        click_track(window, hardness, 0.5, cx);
        window.render_frame(cx);
        assert!((content.read(cx).state_for_test().read(cx).brush_hardness - 0.5).abs() < 0.03);

        let strength = surface_bounds(&content, cx, |state| state.brush_menu_strength_bounds.get());
        click_track(window, strength, 0.5, cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).brush_opacity,
            0.5
        );
        assert!(
            content.read(cx).brush_menu_for_test(cx).is_some(),
            "a slider drag keeps the panel open"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn brush_menu_preset_applies_every_value_and_closes(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 20.0);
        window.render_frame(cx);
        let position = window.find("canvas-hitbox").bounds().center();
        right_click(window, position, cx);
        window.render_frame(cx);
        window.click("brush-menu-preset-1", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.brush_size, 40.0);
            assert!((state.brush_hardness - 0.15).abs() < 1e-9);
            assert_eq!(state.brush_opacity, 1.0);
            assert_eq!(state.brush_flow, 1.0);
            assert_eq!(state.brush_smoothing, 0.0);
            assert_eq!(state.brush_preset_index, 1);
        }
        assert!(content.read(cx).brush_menu_for_test(cx).is_none());
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn brush_menu_reset_restores_the_tool_defaults_and_closes(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Eraser, 72.0);
        window.render_frame(cx);
        let position = window.find("canvas-hitbox").bounds().center();
        right_click(window, position, cx);
        window.render_frame(cx);
        window.click("brush-menu-reset", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.eraser_size, 32.0);
            assert_eq!(state.eraser_hardness, 1.0);
            assert_eq!(state.eraser_opacity, 1.0);
            assert_eq!(state.eraser_flow, 1.0);
            assert_eq!(state.eraser_smoothing, 0.0);
            assert_eq!(state.brush_preset_index, usize::MAX);
        }
        assert!(content.read(cx).brush_menu_for_test(cx).is_none());
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn escape_and_click_away_close_the_brush_menu(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 20.0);
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        let position = window.find("canvas-hitbox").bounds().center();
        right_click(window, position, cx);
        window.render_frame(cx);
        assert!(content.read(cx).brush_menu_for_test(cx).is_some());

        window.press("escape", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).brush_menu_for_test(cx).is_none(),
            "Escape closes the panel"
        );

        right_click(window, position, cx);
        window.render_frame(cx);
        window.click("brush-menu-away", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).brush_menu_for_test(cx).is_none(),
            "a click outside closes the panel"
        );
    })
    .expect("test window remains open");
}

/// The menu's Size and Hardness tracks take a click as a focus, and from there
/// the keyboard reaches the same ends the pointer path maps to.
#[gpui_kit::test]
fn the_brush_menu_tracks_answer_arrow_keys(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 20.0);
        window.render_frame(cx);
        let position = window.find("canvas-hitbox").bounds().center();
        right_click(window, position, cx);
        window.render_frame(cx);

        let bounds = window.find("brush-menu-size").bounds();
        window.click_at(
            "brush-menu-size",
            gpui_kit::point(
                gpui_kit::px(f32::from(bounds.size.width) * 0.5),
                gpui_kit::px(f32::from(bounds.size.height) * 0.5),
            ),
            cx,
        );
        window.render_frame(cx);
        let middle = content.read(cx).state_for_test().read(cx).brush_size;
        assert!(
            (300.0..=370.0).contains(&middle),
            "halfway along the Size track is the curve's 334 px, got {middle}"
        );

        window.press("home", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).brush_size,
            1.0,
            "Home shrinks the brush to its smallest dab"
        );
        window.press("right", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).brush_size,
            8.0,
            "one arrow off the low end widens the dab to the curve's next point"
        );
        window.press("end", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).brush_size,
            2000.0,
            "End gives the full 2000 px the drag path can set"
        );

        let bounds = window.find("brush-menu-hardness").bounds();
        window.click_at(
            "brush-menu-hardness",
            gpui_kit::point(
                gpui_kit::px(f32::from(bounds.size.width) * 0.5),
                gpui_kit::px(f32::from(bounds.size.height) * 0.5),
            ),
            cx,
        );
        window.render_frame(cx);
        window.press("home", cx);
        let state = content.read(cx).state_for_test();
        assert_eq!(
            state.read(cx).brush_hardness,
            0.0,
            "the hardness track has its own handle and reaches its low end"
        );
        assert_eq!(
            state.read(cx).brush_size,
            2000.0,
            "stepping Hardness leaves the Size the keyboard set"
        );
    })
    .expect("test window remains open");
}
