//! The History panel's own behaviour: the core entry list, jumping to an entry, the
//! version a new document starts at, and the latency of a row jump.

use super::*;

#[gpui_kit::test]
fn history_panel_lists_and_jumps_core_entries(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        let (w, h, blank) = pixels(&content, cx);
        let doc = (w, h);
        drag_doc(window, doc, (0.5, 0.5), (1.5, 0.5), cx);
        let (_, _, first) = pixels(&content, cx);
        drag_doc(window, doc, (0.5, 0.5), (1.5, 1.0), cx);
        let (_, _, second) = pixels(&content, cx);
        assert_ne!(first, second);

        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.layer_tab = crate::state::LayerTab::History
            });
        });
        window.render_frame(cx);
        let rows = content.read(cx).state_for_test().read(cx).history.clone();
        assert_eq!(rows.len(), 3, "Open plus two strokes");
        assert_eq!(rows[0].label, "Open");
        assert_eq!(rows[1].label, "Brush");
        assert_eq!(rows[2].label, "Brush");
        assert!(rows[2].current, "the newest entry is current");

        window.click("history-row-1", cx);
        window.render_frame(cx);
        assert_eq!(
            pixels(&content, cx).2,
            first,
            "clicking the first stroke restores its pixels"
        );
        assert!(content
            .read(cx)
            .state_for_test()
            .read(cx)
            .history
            .iter()
            .find(|row| row.current)
            .is_some_and(|row| row.label != "Open"));

        window.click("history-row-2", cx);
        window.render_frame(cx);
        assert_eq!(pixels(&content, cx).2, second);

        window.click("history-row-0", cx);
        window.render_frame(cx);
        assert_eq!(
            pixels(&content, cx).2,
            blank,
            "the Open row reverts every entry"
        );
    })
    .expect("test window remains open");
}

/// A freshly created document starts its history at `Open`:
/// seeding the layer is not an edit and must not add an entry.
#[gpui_kit::test]
fn new_document_history_starts_at_open(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document")
        });
        window.render_frame(cx);
        let rows = content.read(cx).state_for_test().read(cx).history.clone();
        assert_eq!(
            rows.len(),
            1,
            "a new document's stack is Open alone: {rows:?}"
        );
        assert_eq!(rows[0].label, "Open");
        assert!(rows[0].current);
    })
    .expect("test window remains open");
}

/// A history row jump (dispatch + one render), alternating between the
/// two stroke entries so every sample restores a different state.
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn history_row_jump_latency(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::{MouseButton, MouseDownEvent, MouseUpEvent, PlatformInput};
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        let (w, h, _blank) = pixels(&content, cx);
        let doc = (w, h);
        drag_doc(window, doc, (0.5, 0.5), (1.5, 0.5), cx);
        let (_, _, first) = pixels(&content, cx);
        drag_doc(window, doc, (0.5, 0.5), (1.5, 1.0), cx);
        let (_, _, second) = pixels(&content, cx);
        assert_ne!(first, second);

        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.layer_tab = crate::state::LayerTab::History
            });
        });
        window.render_frame(cx);
        // Warm the jump path once; the first click pays the panel mount cost.
        window.click("history-row-1", cx);
        window.render_frame(cx);

        let mut samples = Vec::new();
        let mut next = "history-row-2";
        for sample in 0..9 {
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
            if next == "history-row-2" {
                assert_eq!(pixels(&content, cx).2, second, "sample {sample}");
                next = "history-row-1";
            } else {
                assert_eq!(pixels(&content, cx).2, first, "sample {sample}");
                next = "history-row-2";
            }
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S025_HISTORY_JUMP_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "history row jump p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}
