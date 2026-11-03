use super::*;
use gpui_kit::test::TestWindowExt as _;

/// Open the Print dialog with a pinned printer list. `print_document_action`
/// enumerates this machine's spooler, and a host with no printers gets the
/// warning row in place of the picker, so the inspector's contents would depend
/// on where the test runs. The perf tests keep the real enumeration: it is what
/// they measure.
fn open_print_dialog(content: &Entity<App>, window: &mut gpui_kit::Window, cx: &mut gpui_kit::App) {
    content.update(cx, |app, cx| {
        app.print_document_action(&crate::app::PrintDocument, cx);
        app.state_for_test().update(cx, |state, _| {
            state.printers = vec!["First printer".to_string(), "Second printer".to_string()];
            state.print_error = None;
            state.print_printer = 0;
        });
    });
    window.render_frame(cx);
    assert!(
        window.try_find("print-error").is_none(),
        "a listed printer keeps the warning row out of the inspector"
    );
}

#[gpui_kit::test]
fn print_dialog_controls_paper_orientation_and_copies(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        open_print_dialog(&content, window, cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            Some(crate::state::DialogId::Print)
        );
        // The paper list is inline behind its select, like the dialog's.
        window.click("print-paper", cx);
        window.render_frame(cx);
        window.click("print-paper-1", cx);
        window.render_frame(cx);
        window.click("print-orientation-landscape", cx);
        window.render_frame(cx);
        window.click("print-copies-inc", cx);
        window.render_frame(cx);
        window.click("print-unit", cx);
        window.render_frame(cx);
        window.click("print-unit-cm", cx);
        window.render_frame(cx);
        window.click("print-center", cx);
        window.render_frame(cx);
        window.click("print-scale-fit", cx);
        window.render_frame(cx);
        window.click("print-paper-white", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.print_paper, 1);
            assert!(state.print_landscape);
            assert_eq!(state.print_copies, 2);
            assert_eq!(state.print_unit, "cm");
            assert!(!state.print_center_on_page, "Center on Page toggles");
            assert!(!state.print_scale_to_fit, "Scale to Fit toggles");
            assert!(!state.print_show_paper_white, "Show Paper White toggles");
        }
        window.click("print-cancel", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            None
        );
    })
    .expect("test window remains open");
}

/// The Print dialog's Scale track writes the percentage (10..=400) and drops
/// Scale to Fit, matching the dialog's range input (min 10, max 400).
#[gpui_kit::test]
fn print_scale_track_writes_the_percentage(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        open_print_dialog(&content, window, cx);
        let bounds = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .print_scale_slider_bounds
            .get()
            .expect("the Scale track is laid out");
        let x = bounds.origin.x + gpui_kit::px(f32::from(bounds.size.width) * 0.5);
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
        let state = content.read(cx).state_for_test();
        let scale = state.read(cx).print_scale_percent;
        assert!(
            (200..=205).contains(&scale),
            "halfway is about 205 %, got {scale}"
        );
        assert!(
            !state.read(cx).print_scale_to_fit,
            "dragging the Scale track clears Scale to Fit"
        );
    })
    .expect("test window remains open");
}

/// The Scale track takes a click as a focus, and from there the keyboard covers
/// the same 10..=400 range the pointer path maps, clearing Scale to Fit.
#[gpui_kit::test]
fn the_print_scale_track_answers_arrow_keys(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        open_print_dialog(&content, window, cx);

        let bounds = window.find("print-scale-track").bounds();
        window.click_at(
            "print-scale-track",
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
            .print_scale_percent;
        assert!(
            (195..=215).contains(&centre),
            "the click that focuses the track maps to about 205 %, got {centre}"
        );

        window.press("right", cx);
        window.press("up", cx);
        assert_eq!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .print_scale_percent,
            centre + 2,
            "arrows step the scale up one percent each"
        );
        window.press("home", cx);
        assert_eq!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .print_scale_percent,
            10,
            "Home reaches the track's 10 % floor"
        );
        window.press("end", cx);
        let state = content.read(cx).state_for_test();
        assert_eq!(
            state.read(cx).print_scale_percent,
            400,
            "End reaches the track's 400 % ceiling"
        );
        assert!(
            !state.read(cx).print_scale_to_fit,
            "keyboard steps leave Fit cleared, as a drag does"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn print_dialog_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let close = |content: &gpui_kit::Entity<App>, cx: &mut gpui_kit::App| {
            content.update(cx, |app, cx| {
                app.state_for_test().update(cx, |state, _| {
                    state.active_dialog = None;
                });
            });
        };
        close(&content, cx);
        window.render_frame(cx);
        let mut samples = Vec::new();
        for _ in 0..9 {
            close(&content, cx);
            window.render_frame(cx);
            let start = std::time::Instant::now();
            content.update(cx, |app, cx| {
                app.print_document_action(&crate::app::PrintDocument, cx);
            });
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "print_dialog p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "print dialog p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn print_scale_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.print_document_action(&crate::app::PrintDocument, cx);
        });
        window.render_frame(cx);
        let bounds = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .print_scale_slider_bounds
            .get()
            .expect("the Scale track is laid out");
        let mut samples = Vec::new();
        for index in 0..9 {
            let fraction = if index % 2 == 0 { 0.3 } else { 0.7 };
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
            "print_scale p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "print scale p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}
