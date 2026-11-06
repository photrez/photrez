use super::*;
use gpui_kit::test::TestWindowExt as _;

fn slider_bounds(
    app: &gpui_kit::Entity<App>,
    cx: &mut gpui_kit::App,
    index: usize,
) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    app.read(cx)
        .state_for_test()
        .read(cx)
        .adjustment_slider_bounds[index]
        .get()
        .expect("the adjusted track is laid out")
}

fn drag_track(
    window: &mut gpui_kit::Window,
    bounds: gpui_kit::Bounds<gpui_kit::Pixels>,
    fraction: f32,
    cx: &mut gpui_kit::App,
) {
    let x = bounds.origin.x + gpui_kit::px(f32::from(bounds.size.width) * fraction);
    let y = bounds.origin.y + bounds.size.height / 2.0;
    let _ = window.dispatch_event(
        gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
            button: gpui_kit::MouseButton::Left,
            position: gpui_kit::point(x, y),
            ..Default::default()
        }),
        cx,
    );
    let _ = window.dispatch_event(
        gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
            button: gpui_kit::MouseButton::Left,
            position: gpui_kit::point(x, y),
            click_count: 1,
            ..Default::default()
        }),
        cx,
    );
    window.render_frame(cx);
}

/// The panel's Bright track writes the selected layer's basic adjustment
/// through core history, and Reset clears it again.
#[gpui_kit::test]
fn adjustments_panel_slider_writes_the_layer_adjustment(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        // The dock shows the panel once the Adjustments tab is active.
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.inspector_tab = crate::state::InspectorTab::Adjustments;
            });
        });
        window.render_frame(cx);

        let bright = slider_bounds(&content, cx, 0);
        drag_track(window, bright, 0.25, cx);
        let value = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .adjustment_brightness;
        assert_eq!(
            value, -50.0,
            "a press at 25 % of the centre-origin track writes -50"
        );

        let contrast = slider_bounds(&content, cx, 1);
        drag_track(window, contrast, 0.75, cx);
        let state = content.read(cx).state_for_test();
        assert_eq!(state.read(cx).adjustment_contrast, 50.0);
        assert_eq!(
            state.read(cx).adjustment_brightness,
            -50.0,
            "dragging another channel keeps the first one"
        );

        let reset = window.find("adj-reset").bounds().center();
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                button: gpui_kit::MouseButton::Left,
                position: reset,
                ..Default::default()
            }),
            cx,
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                button: gpui_kit::MouseButton::Left,
                position: reset,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        let state = content.read(cx).state_for_test();
        assert_eq!(
            state.read(cx).adjustment_brightness,
            0.0,
            "Reset clears Bright"
        );
        assert_eq!(
            state.read(cx).adjustment_contrast,
            0.0,
            "Reset clears Contrast"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn adjustment_slider_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.inspector_tab = crate::state::InspectorTab::Adjustments;
            });
        });
        window.render_frame(cx);
        let bounds = slider_bounds(&content, cx, 0);
        let mut samples = Vec::new();
        for index in 0..9 {
            let fraction = if index % 2 == 0 { 0.25 } else { 0.75 };
            let x = bounds.origin.x + gpui_kit::px(f32::from(bounds.size.width) * fraction);
            let y = bounds.origin.y + bounds.size.height / 2.0;
            let start = std::time::Instant::now();
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                    button: gpui_kit::MouseButton::Left,
                    position: gpui_kit::point(x, y),
                    ..Default::default()
                }),
                cx,
            );
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                    button: gpui_kit::MouseButton::Left,
                    position: gpui_kit::point(x, y),
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
            "adjustment_slider p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "adjustment slider p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

/// The five not-yet-built adjustment groups are headers that open. Each one
/// reveals its In Development card, and opening a group is
/// independent of the others.
#[gpui_kit::test]
fn undeveloped_adjustment_groups_open_their_in_development_card(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        // The dock shows the panel once the Adjustments tab is active.
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.inspector_tab = crate::state::InspectorTab::Adjustments;
            });
        });
        window.render_frame(cx);
        assert!(
            window.try_find("adj-card-adj-tone-curve").is_none(),
            "a collapsed group shows its header only"
        );

        window.click("adj-tone-curve", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("adj-card-adj-tone-curve").is_some(),
            "the header opens the In Development card"
        );
        let card = window.find("adj-card-adj-tone-curve");
        let height = f32::from(card.bounds().size.height);
        assert!(
            height > 60.0,
            "the expanded card is a real block, not a clipped row: {height}"
        );

        window.click("adj-hsl-color", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("adj-card-adj-tone-curve").is_some()
                && window.try_find("adj-card-adj-hsl-color").is_some(),
            "opening one group leaves the other group's state alone"
        );

        window.click("adj-tone-curve", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("adj-card-adj-tone-curve").is_none(),
            "the header closes the group again"
        );
        assert!(
            window.try_find("adj-card-adj-hsl-color").is_some(),
            "closing one group does not close the other"
        );
        let open = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .open_adjustment_sections
            .clone();
        assert_eq!(
            open,
            vec!["adj-hsl-color".to_string()],
            "the panel keeps only the group still expanded"
        );
    })
    .expect("test window remains open");
}

/// A click focuses a track, and from there the arrows step one point and
/// Home/End reach the -100/+100 ends the pointer path maps to.
#[gpui_kit::test]
fn the_adjustment_tracks_answer_arrow_keys(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.inspector_tab = crate::state::InspectorTab::Adjustments;
            });
        });
        window.render_frame(cx);

        let bounds = window.find("adj-bright-track").bounds();
        window.click_at(
            "adj-bright-track",
            gpui_kit::point(
                gpui_kit::px(f32::from(bounds.size.width) * 0.5),
                gpui_kit::px(f32::from(bounds.size.height) * 0.5),
            ),
            cx,
        );
        window.render_frame(cx);
        let centre = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .adjustment_brightness;
        assert!(
            centre.abs() <= 2.0,
            "the click that focuses the track sits at its centre, got {centre}"
        );

        window.press("right", cx);
        window.press("up", cx);
        let raised = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .adjustment_brightness;
        assert!(
            (raised - centre - 2.0).abs() <= 0.01,
            "arrows step the channel up by two points, got {raised} from {centre}"
        );

        window.press("home", cx);
        assert_eq!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .adjustment_brightness,
            -100.0,
            "Home drives Bright to its low end"
        );
        window.press("end", cx);
        assert_eq!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .adjustment_brightness,
            100.0,
            "End drives Bright to its high end"
        );

        let bounds = window.find("adj-saturate-track").bounds();
        window.click_at(
            "adj-saturate-track",
            gpui_kit::point(
                gpui_kit::px(f32::from(bounds.size.width) * 0.5),
                gpui_kit::px(f32::from(bounds.size.height) * 0.5),
            ),
            cx,
        );
        window.render_frame(cx);
        window.press("end", cx);
        let state = content.read(cx).state_for_test();
        assert_eq!(
            state.read(cx).adjustment_saturation,
            100.0,
            "the second channel has its own handle"
        );
        assert_eq!(
            state.read(cx).adjustment_brightness,
            100.0,
            "stepping Saturate leaves Bright where End put it"
        );
    })
    .expect("test window remains open");
}
