//! Mounted coverage for the Export dialog: format choice, the quality track's
//! pointer and keyboard paths, and the latency of opening it.

use super::*;
use gpui_kit::test::TestWindowExt as _;
#[gpui_kit::test]
fn export_dialog_chooses_format_and_quality(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.export_document_action(&crate::app::ExportDocument, cx);
            assert_eq!(
                app.state_for_test().read(cx).active_dialog,
                Some(crate::state::DialogId::Export)
            );
        });
        window.render_frame(cx);
        window.click("export-format-trigger", cx);
        window.render_frame(cx);
        window.click("export-format-jpg", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.export_format, crate::state::ExportFormat::Jpeg);
            assert_eq!(state.export_quality, 90);
            assert_eq!(state.active_dialog, Some(crate::state::DialogId::Export));
        }

        window.click("export-cancel", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            None
        );
    })
    .expect("test window remains open");
}

/// The Export dialog's behaviour: the format list swaps
/// PNG/JPEG/WebP, JPEG reveals the quality track, PNG hides it, and a press on
/// the track writes the quality.
#[gpui_kit::test]
fn export_dialog_switches_format_and_quality(cx: &mut gpui_kit::TestAppContext) {
    use crate::state::ExportFormat;

    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.export_document_action(&crate::app::ExportDocument, cx);
        });
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            Some(crate::state::DialogId::Export),
            "the Export action opens the dialog"
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).export_format,
            ExportFormat::Png
        );
        assert!(
            window.try_find("export-quality").is_none(),
            "PNG has no quality track"
        );
        window.click("export-format-trigger", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("export-format-jpg").is_some(),
            "the format list opens under the trigger"
        );
        window.click("export-format-jpg", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).export_format,
            ExportFormat::Jpeg
        );
        assert!(
            !content
                .read(cx)
                .state_for_test()
                .read(cx)
                .export_format_open,
            "picking a format closes the list"
        );
        assert!(
            window.try_find("export-quality").is_some(),
            "JPEG shows the quality track"
        );
        let bounds = window.find("export-quality").bounds();
        let point = gpui_kit::Point {
            x: bounds.origin.x + bounds.size.width * 0.25,
            y: bounds.origin.y + bounds.size.height / 2.0,
        };
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                button: gpui_kit::MouseButton::Left,
                position: point,
                ..Default::default()
            }),
            cx,
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                button: gpui_kit::MouseButton::Left,
                position: point,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        let quality = content.read(cx).state_for_test().read(cx).export_quality;
        assert!(
            (20..=30).contains(&quality),
            "a press a quarter into the track sets ~26%, got {quality}"
        );
        window.click("export-format-trigger", cx);
        window.render_frame(cx);
        window.click("export-format-png", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("export-quality").is_none(),
            "PNG hides the track again"
        );
    })
    .expect("test window remains open");
}

/// The quality track answers the keyboard once a click has focused it: arrows
/// step one percent and Home/End reach the 1..=100 ends of the range input.
#[gpui_kit::test]
fn the_export_quality_track_answers_arrow_keys(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.export_document_action(&crate::app::ExportDocument, cx);
        });
        window.render_frame(cx);
        window.click("export-format-trigger", cx);
        window.render_frame(cx);
        window.click("export-format-jpg", cx);
        window.render_frame(cx);

        let bounds = window.find("export-quality").bounds();
        window.click_at(
            "export-quality",
            gpui_kit::point(
                gpui_kit::px(f32::from(bounds.size.width) * 0.5),
                gpui_kit::px(f32::from(bounds.size.height) * 0.5),
            ),
            cx,
        );
        window.render_frame(cx);
        let centre = content.read(cx).state_for_test().read(cx).export_quality;
        assert!(
            (45..=55).contains(&centre),
            "the click that focuses the track maps to about 51 %, got {centre}"
        );

        window.press("right", cx);
        window.press("up", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).export_quality,
            centre + 2,
            "arrows step the quality up one percent each"
        );
        window.press("home", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).export_quality,
            1,
            "Home reaches the track's lowest quality"
        );
        window.press("end", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).export_quality,
            100,
            "End reaches full quality"
        );
    })
    .expect("test window remains open");
}

/// Perf: opening the export dialog and painting its body.
#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn export_dialog_latency(cx: &mut gpui_kit::TestAppContext) {
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
                app.export_document_action(&crate::app::ExportDocument, cx);
            });
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "export_dialog p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "export dialog open p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

/// Tab traversal reaches content inside an open dialog: this track is a tab stop even
/// while the Export dialog is up, and the keys then write through it. What this does
/// NOT cover is the dialog's own fields, which carry no handle at all and so never
/// enter the tab order.
#[gpui_kit::test]
fn the_export_quality_track_is_reachable_by_tab(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.export_document_action(&crate::app::ExportDocument, cx);
        });
        window.render_frame(cx);
        window.click("export-format-trigger", cx);
        window.render_frame(cx);
        window.click("export-format-jpg", cx);
        window.render_frame(cx);
        let mut reached = false;
        for _ in 0..40 {
            window.press("tab", cx);
            window.render_frame(cx);
            if window.find("export-quality").focused() == Some(true) {
                reached = true;
                break;
            }
        }
        assert!(reached, "Tab must reach the track inside the open dialog");

        let before = content.read(cx).state_for_test().read(cx).export_quality;
        window.press("right", cx);
        window.press("right", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).export_quality,
            before + 2,
            "arrows step the dialog track that Tab focused"
        );
    })
    .expect("test window remains open");
}
