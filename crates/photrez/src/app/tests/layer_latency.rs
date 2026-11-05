// Latency probes for the Layers panel. Each one is `#[ignore]`d and run explicitly,
// because parallel load inflates the p95: the eye toggle, the Properties refresh after a
// lock, the guarded background-lock click plus its toast, and the blend control press.
use super::input::{mount_archive_app, select_layer};
use super::layers_panel::flags;
use super::*;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{MouseButton, MouseDownEvent, MouseUpEvent, PlatformInput};

/// The layer row eye toggle response (dispatch + one render).
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
            "LAYER_EYE_MS samples={samples:?} p50={:.3} p95={:.3}",
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

/// The Properties status refresh after a Layers lock toggle.
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
            "PROPERTIES_REFRESH_MS samples={samples:?} p50={:.3} p95={:.3}",
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

/// The guarded row-lock click plus the toast paint (dispatch + render).
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
            "TOAST_MS samples={samples:?} p50={:.3} p95={:.3}",
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

/// The blend control's click -> composited frame budget: one press/release on
/// the Layers select plus the frame that repaints the canvas with the new
/// blend mode.
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn blend_cycle_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, _content, _source) = mount_archive_app(
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
