use super::input::{drag_doc, pixels, set_tool};
use super::*;
use crate::app::history::{RedoDocument, UndoDocument};
use crate::state::ToolId;
use gpui_kit::test::TestWindowExt as _;

/// The core history cursor drives the page: undoing a drawn shape restores the
/// white Background composite and the page-sized selection frame, and redo puts
/// the shape back.
#[gpui_kit::test]
fn undo_restores_the_page_and_redo_restores_the_shape(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 200, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Shape, 1.0);
        drag_doc(window, (400, 200), (50.0, 50.0), (150.0, 120.0), cx);
        window.render_frame(cx);
        let layers_after_draw =
            content.update(cx, |app, cx| app.state_for_test().read(cx).layers.len());
        assert_eq!(layers_after_draw, 2, "the shape adds a second layer");

        content.update(cx, |app, cx| {
            app.undo_document_action(&UndoDocument, cx);
        });
        window.render_frame(cx);
        let (selected, layers) = content.update(cx, |app, cx| {
            let state = app.state_for_test().read(cx);
            (
                state
                    .layers
                    .iter()
                    .find(|row| row.id == state.selected_layer)
                    .map(|row| row.name.clone()),
                state.layers.len(),
            )
        });
        assert_eq!(layers, 1, "undo removes the drawn shape");
        assert_eq!(
            selected.as_deref(),
            Some("Background"),
            "undo keeps the remaining layer selected"
        );
        let (width, height, rgba) = pixels(&content, cx);
        let center =
            ((u32::from(height) / 2 * u32::from(width) + u32::from(width) / 2) as usize) * 4;
        assert_eq!(
            &rgba[center..center + 4],
            [255, 255, 255, 255],
            "the page repaints the white Background after undo"
        );

        content.update(cx, |app, cx| {
            app.redo_document_action(&RedoDocument, cx);
        });
        window.render_frame(cx);
        let restored = content.update(cx, |app, cx| app.state_for_test().read(cx).layers.len());
        assert_eq!(restored, 2, "redo restores the shape layer");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn history_undo_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 200, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Shape, 1.0);
        drag_doc(window, (400, 200), (50.0, 50.0), (150.0, 120.0), cx);
        window.render_frame(cx);
        let mut samples = Vec::new();
        for index in 0..9 {
            let start = std::time::Instant::now();
            content.update(cx, |app, cx| {
                if index % 2 == 0 {
                    app.undo_document_action(&UndoDocument, cx);
                } else {
                    app.redo_document_action(&RedoDocument, cx);
                }
            });
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "history_undo p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "history undo p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}
