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

/// Clicking the thumb overhang above the 4px track line jumps the slider
/// too: the hitbox covers the visible 12px thumb, not just the line.
#[gpui_kit::test]
fn clicking_the_thumb_overhang_jumps_the_slider(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.inspector_tab = crate::state::InspectorTab::Adjustments;
            });
        });
        window.render_frame(cx);
        let bright = slider_bounds(&content, cx, 0);
        let x = bright.origin.x + gpui_kit::px(f32::from(bright.size.width) * 0.25);
        // 4px above the stored 4px-tall track bounds: inside the thumb
        // overhang, outside the old line-only hitbox.
        let y = bright.origin.y - gpui_kit::px(4.0);
        let position = gpui_kit::point(x, y);
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
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        let value = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .adjustment_brightness;
        assert_eq!(
            value, -50.0,
            "a press in the thumb overhang jumps the track to 25%"
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

/// Pointer moves during a drag show a preview without touching core; the
/// release commits once. A drag at the current value composites nothing.
#[gpui_kit::test]
fn slider_moves_coalesce_to_one_composite_per_frame(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let mut initial_composites = 0;
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.inspector_tab = crate::state::InspectorTab::Adjustments;
            });
        });
        window.render_frame(cx);
        let bounds = slider_bounds(&content, cx, 0);
        let before = content.read(cx).full_composites_for_test();
        let y = bounds.origin.y + bounds.size.height / 2.0;
        let at = |fraction: f32| {
            gpui_kit::point(
                bounds.origin.x + gpui_kit::px(f32::from(bounds.size.width) * fraction),
                y,
            )
        };
        let down = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App, fraction: f32| {
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                    button: gpui_kit::MouseButton::Left,
                    position: at(fraction),
                    ..Default::default()
                }),
                cx,
            );
        };
        let mov = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App, fraction: f32| {
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                    position: at(fraction),
                    ..Default::default()
                }),
                cx,
            );
        };
        let up = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App, fraction: f32| {
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                    button: gpui_kit::MouseButton::Left,
                    position: at(fraction),
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
        };
        down(window, cx, 0.60);
        mov(window, cx, 0.70);
        mov(window, cx, 0.75);
        window.render_frame(cx);
        // The coalesced value lands on the frame after the callback runs.
        let _ = window.simulate_next_frame(cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).full_composites_for_test(),
            before,
            "during the drag the preview patches; core is not recomposited"
        );
        assert_eq!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .adjustment_brightness,
            50.0,
            "the thumb follows the preview before any commit"
        );
        initial_composites = before;
        up(window, cx, 0.75);
        window.render_frame(cx);
    })
    .expect("test window remains open");
    cx.run_until_parked();
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let _ = window.simulate_next_frame(cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).full_composites_for_test(),
            initial_composites + 1,
            "the release commits the previewed value exactly once"
        );
        assert_eq!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .adjustment_brightness,
            50.0,
            "the committed value survives the release"
        );
        // A drag at the current value must not composite.
        let still = content.read(cx).full_composites_for_test();
        let bounds = slider_bounds(&content, cx, 0);
        let y = bounds.origin.y + bounds.size.height / 2.0;
        let same = gpui_kit::point(
            bounds.origin.x + gpui_kit::px(f32::from(bounds.size.width) * 0.75),
            y,
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                button: gpui_kit::MouseButton::Left,
                position: same,
                ..Default::default()
            }),
            cx,
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: same,
                ..Default::default()
            }),
            cx,
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                button: gpui_kit::MouseButton::Left,
                position: same,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).full_composites_for_test(),
            still,
            "a move at the current value is a no-op for the canvas"
        );
    })
    .expect("test window remains open");
}

/// With no document open the panel shows the centered "No image open" state
/// instead of the Selected Layer box, the Basic sliders and the coming-soon
/// cards — the same no-document branch the Properties panel already mounts.
///
/// RED proof: drop the `!has_document` branch in `views/adjustments_panel.rs`
/// and the first assert fails on the slider section still being mounted.
#[gpui_kit::test]
fn adjustments_panel_shows_the_no_image_state_without_a_document(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (window, content) = mount_app(cx, None, None);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.inspector_tab = crate::state::InspectorTab::Adjustments;
            });
        });
        window.render_frame(cx);

        assert!(
            window.try_find("adjustments-no-image").is_some(),
            "the panel mounts its no-image state without a document"
        );
        assert!(
            window.try_find("adj-reset").is_none(),
            "the Basic section stays off while no document is open"
        );
    })
    .expect("test window remains open");
}

/// Opening accordions past the panel's height makes the column scroll: a wheel
/// over the panel moves the sections up while their x positions stay put, so
/// the scrollbar never shifts the layout sideways.
///
/// RED proof: against the clipped, unscrollable column the wheel moves
/// nothing and the last section's y stays put.
#[gpui_kit::test]
fn expanded_adjustment_sections_scroll_under_the_wheel(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.inspector_tab = crate::state::InspectorTab::Adjustments;
                state.open_adjustment_sections = vec![
                    "adj-tone-curve".to_string(),
                    "adj-hsl-color".to_string(),
                    "adj-color-grading".to_string(),
                    "adj-detail".to_string(),
                ];
            });
        });
        window.render_frame(cx);

        let last = window.find("adj-lens-corrections").bounds();
        let panel = window.find("inspector-body-adjustments").bounds();
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::Scroll(gpui_kit::ScrollEvent {
                position: panel.center(),
                delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(
                    gpui_kit::px(0.),
                    gpui_kit::px(-160.),
                )),
                modifiers: Default::default(),
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);

        let scrolled = window.find("adj-lens-corrections").bounds();
        assert!(
            scrolled.origin.y < last.origin.y,
            "the wheel must scroll the expanded column up: {last:?} -> {scrolled:?}"
        );
        assert_eq!(
            scrolled.origin.x, last.origin.x,
            "scrolling must not shift the sections sideways"
        );
    })
    .expect("test window remains open");
}
