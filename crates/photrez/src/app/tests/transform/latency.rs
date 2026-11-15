//! The measured costs: a click on the empty canvas and the release of a Move drag.

use super::*;

/// A left press and release on the empty canvas page clears the
/// selection and repaints. The measured span is the dispatch pair plus one
/// frame; reseeding the selection between samples is host-side.
#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn canvas_click_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let (a, b) = draw_two_shapes(window, &content, cx);
        let (bounds, placement) = page(window, (200, 150));
        let empty = super::canvas_input::window_point(bounds, placement, (200, 150), 5.0, 140.0);
        let press = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                    button: gpui_kit::MouseButton::Left,
                    position: empty,
                    ..Default::default()
                }),
                cx,
            );
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                    button: gpui_kit::MouseButton::Left,
                    position: empty,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
        };
        select_rows(&content, cx, vec![a, b]);
        window.render_frame(cx);
        press(window, cx);
        window.render_frame(cx);
        let mut samples = Vec::new();
        for _ in 0..9 {
            select_rows(&content, cx, vec![a, b]);
            window.render_frame(cx);
            let start = std::time::Instant::now();
            press(window, cx);
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format("{s:.4}")).collect();
        println!(
            "canvas_click p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "canvas click p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}
/// Perf: one Move drag's release, i.e. the transform commit plus the frame
/// that repaints the moved layer. The press and the preview frames are warm-up;
/// the measured event is release -> visible result.
#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn transform_commit_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let (a, _b) = draw_two_shapes(window, &content, cx);
        select_rows(&content, cx, vec![a]);
        window.render_frame(cx);
        let (bounds, placement) = page(window, (200, 150));
        let mut samples = Vec::new();
        for _ in 0..9 {
            let (x, y, _, _, _) = transform(&content, cx);
            let center = (x + 35.0, y + 35.0);
            let from = super::canvas_input::window_point(
                bounds,
                placement,
                (200, 150),
                center.0 as f32,
                center.1 as f32,
            );
            let to = super::canvas_input::window_point(
                bounds,
                placement,
                (200, 150),
                (center.0 + 6.0) as f32,
                (center.1 + 4.0) as f32,
            );
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                    button: gpui_kit::MouseButton::Left,
                    position: from,
                    ..Default::default()
                }),
                cx,
            );
            let start = std::time::Instant::now();
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                    button: gpui_kit::MouseButton::Left,
                    position: to,
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
        let listed: Vec<String> = samples.iter().map(|s| format("{s:.4}")).collect();
        println!(
            "transform_commit p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "transform commit p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}
