// The three selection latency legs: a cross-document paste, a marquee commit and a
// rotation commit. Each is `#[ignore]`d and run serially on purpose, because parallel
// load inflates the p95, and each times a dispatch plus the committed frame.

use super::canvas_input::{page, set_tool, tab_to_canvas, window_point};
use super::selection::{dispatch, set_selection};
use super::*;
use crate::state::ToolId;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput};

/// The cross-document paste's event-to-frame latency.
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn cross_document_paste_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 200, cx).expect("blank document");
        });
        // Build the clipboard once: a 3x2 selection over the painted pixels.
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        let (bounds, page) = page(window, (400, 200));
        let at = |fx: f32, fy: f32| window_point(bounds, page, (400, 200), fx, fy);
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: at(60.0, 40.0),
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: at(120.0, 90.0),
                ..Default::default()
            }),
            cx,
        );
        window.press("ctrl-c", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            app.create_document(300, 300, cx).expect("target document");
        });
        set_tool(&content, cx, ToolId::Selection, 1.0);
        tab_to_canvas(window, cx);
        window.render_frame(cx);

        // One untimed warm-up paste (the shared measurement protocol).
        window.press("ctrl-v", cx);
        window.render_frame(cx);
        window.press("ctrl-z", cx);
        window.render_frame(cx);

        let mut samples = Vec::new();
        for _sample in 0..9 {
            let start = std::time::Instant::now();
            window.press("ctrl-v", cx);
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            window.press("ctrl-z", cx);
            window.render_frame(cx);
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "CROSS_DOC_PASTE_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "cross-document paste p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}

/// The marquee release commit's event-to-frame latency. Each sample
/// dispatches down/move/up through the production canvas path and times one
/// render, then verifies the selection actually committed.
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn selection_marquee_commit_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 200, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        let (bounds, page) = page(window, (400, 200));
        let start_at = window_point(bounds, page, (400, 200), 80.0, 40.0);
        let end_at = window_point(bounds, page, (400, 200), 320.0, 160.0);

        // One untimed warm-up commit so the first timed sample is not the
        // cold shader/allocator path (the shared measurement protocol).
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start_at,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end_at,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);

        let mut samples = Vec::new();
        for sample in 0..9 {
            window.press("ctrl-d", cx);
            window.render_frame(cx);
            assert!(
                content.read(cx).committed_selection().is_none(),
                "sample {sample} starts deselected"
            );
            let start = std::time::Instant::now();
            dispatch(
                window,
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position: start_at,
                    ..Default::default()
                }),
                cx,
            );
            dispatch(
                window,
                PlatformInput::MouseMove(MouseMoveEvent {
                    position: end_at,
                    ..Default::default()
                }),
                cx,
            );
            dispatch(
                window,
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position: end_at,
                    ..Default::default()
                }),
                cx,
            );
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            let selection = content
                .read(cx)
                .committed_selection()
                .expect("the marquee commits each sample");
            assert!(
                selection.width > 100.0,
                "sample {sample} committed the drag rect"
            );
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "MARQUEE_COMMIT_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "selection marquee commit p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}

/// The rotate-pin release commit's event-to-frame latency.
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn selection_rotation_commit_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 200, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Selection, 1.0);
        set_selection(&content, cx, 100.0, 80.0, 200.0, 80.0);
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        window.press("ctrl-t", cx);
        window.render_frame(cx);

        let (bounds, page) = page(window, (400, 200));
        let scale = f64::from(page.2) / 400.0;
        let pin_doc = (200.0, 80.0 - 24.0 / scale);
        let centre = (200.0, 120.0);
        let radius = centre.1 - pin_doc.1;
        let from = window_point(bounds, page, (400, 200), pin_doc.0 as f32, pin_doc.1 as f32);
        let target = (-60.0_f64).to_radians();
        let to = window_point(
            bounds,
            page,
            (400, 200),
            (centre.0 + radius * target.cos()) as f32,
            (centre.1 + radius * target.sin()) as f32,
        );

        // One untimed warm-up rotation (the shared measurement protocol).
        set_selection(&content, cx, 100.0, 80.0, 200.0, 80.0);
        window.render_frame(cx);
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: from,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: to,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);

        let mut samples = Vec::new();
        for _sample in 0..9 {
            // The pin travels with the rotation; restore the unrotated
            // selection so every sample presses the same pin.
            set_selection(&content, cx, 100.0, 80.0, 200.0, 80.0);
            window.render_frame(cx);
            let start = std::time::Instant::now();
            dispatch(
                window,
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position: from,
                    ..Default::default()
                }),
                cx,
            );
            dispatch(
                window,
                PlatformInput::MouseMove(MouseMoveEvent {
                    position: to,
                    ..Default::default()
                }),
                cx,
            );
            dispatch(
                window,
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position: to,
                    ..Default::default()
                }),
                cx,
            );
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            let angle = content
                .read(cx)
                .core_selection_for_test()
                .expect("selection")
                .angle;
            assert!(
                (angle - 30.0).abs() < 1.0,
                "the sample rotates back to 30 degrees, got {angle}"
            );
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "ROTATE_COMMIT_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "selection rotation commit p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}
