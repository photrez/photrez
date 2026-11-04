// Clipboard arms over a live selection: copy and paste round-trip the selected pixels,
// cut clears the mask and undo brings the pixels back, delete clears only masked pixels,
// and a paste into a second document adds the clipboard there.

use super::input::{history_cursor, pixels};
use super::selection::{history_rows, set_selection};
use super::*;
use gpui_kit::test::TestWindowExt as _;

#[gpui_kit::test]
fn copy_and_paste_round_trips_selected_pixels(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (_, _, before) = pixels(&content, cx);
        set_selection(&content, cx, 0.0, 0.0, 1.0, 1.0);
        let cursor_before = history_cursor(&content, cx);
        let layers_before = content.read(cx).active_core_layer_count_for_test().unwrap();

        content.update(cx, |app, cx| {
            app.copy_selection(cx).expect("copy succeeds");
        });
        assert_eq!(
            content.read(cx).clipboard_for_test(),
            Some((1, 1, before[0..4].to_vec()))
        );
        assert_eq!(
            pixels(&content, cx).2,
            before,
            "copy must not change the layer"
        );
        assert_eq!(
            history_cursor(&content, cx),
            cursor_before,
            "copy is not a history entry"
        );

        content.update(cx, |app, cx| {
            app.paste_selection(cx).expect("paste succeeds");
        });
        let layers_after = content.read(cx).active_core_layer_count_for_test().unwrap();
        assert_eq!(layers_after, layers_before + 1);
        let ids = content.read(cx).active_core_layer_ids_for_test();
        let pasted_id = ids
            .iter()
            .find(|id| id.starts_with("native-layer-"))
            .expect("the pasted layer has a generated id")
            .clone();
        let (width, height, pasted) = content
            .read(cx)
            .core_layer_pixels_for_test(&pasted_id)
            .expect("the pasted layer owns pixels");
        assert_eq!((width, height), (1, 1));
        assert_eq!(pasted, before[0..4].to_vec());
        assert_eq!(history_cursor(&content, cx), cursor_before + 1);
        let (labels, current) = history_rows(&content, cx);
        assert_eq!(labels.last().map(String::as_str), Some("Paste"));
        assert_eq!(current, Some(labels.len() - 1));

        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        assert_eq!(
            content.read(cx).active_core_layer_count_for_test().unwrap(),
            layers_before,
            "undo removes the pasted layer"
        );
        content.update(cx, |app, cx| {
            app.step_history(false, cx).expect("redo succeeds");
        });
        assert_eq!(
            content.read(cx).active_core_layer_count_for_test().unwrap(),
            layers_before + 1,
            "redo restores the pasted layer"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn cut_clears_mask_and_undo_restores_pixels(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (_, _, before) = pixels(&content, cx);
        set_selection(&content, cx, 0.0, 0.0, 1.0, 1.0);
        let cursor_before = history_cursor(&content, cx);

        content.update(cx, |app, cx| {
            app.cut_selection(cx).expect("cut succeeds");
        });
        let (_, _, cut) = pixels(&content, cx);
        assert_eq!(&cut[0..4], &[0, 0, 0, 0], "the selected pixel is cleared");
        assert_eq!(
            &cut[4..8],
            &before[4..8],
            "pixels outside the selection are untouched"
        );
        assert_eq!(
            content.read(cx).clipboard_for_test(),
            Some((1, 1, before[0..4].to_vec()))
        );
        assert_eq!(history_cursor(&content, cx), cursor_before + 1);
        let (labels, _) = history_rows(&content, cx);
        assert_eq!(labels.last().map(String::as_str), Some("Cut"));

        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        assert_eq!(
            pixels(&content, cx).2,
            before,
            "undo restores the cut pixels exactly"
        );
        content.update(cx, |app, cx| {
            app.step_history(false, cx).expect("redo succeeds");
        });
        assert_eq!(
            pixels(&content, cx).2,
            cut,
            "redo re-applies the cut exactly"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn delete_requires_a_selection_and_clears_only_masked_pixels(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (_, _, before) = pixels(&content, cx);
        let cursor_before = history_cursor(&content, cx);

        content.update(cx, |app, cx| {
            let error = app
                .delete_selection(cx)
                .expect_err("delete without a selection is refused");
            assert!(error.contains("no active selection"), "{error}");
        });
        assert_eq!(pixels(&content, cx).2, before);
        assert_eq!(history_cursor(&content, cx), cursor_before);

        set_selection(&content, cx, 0.0, 0.0, 1.0, 1.0);
        content.update(cx, |app, cx| {
            app.delete_selection(cx).expect("delete succeeds");
        });
        let (_, _, deleted) = pixels(&content, cx);
        assert_eq!(&deleted[0..4], &[0, 0, 0, 0]);
        assert_eq!(&deleted[4..8], &before[4..8]);
        assert_eq!(
            content.read(cx).clipboard_for_test(),
            None,
            "delete must not fill the clipboard"
        );
        let (labels, _) = history_rows(&content, cx);
        assert_eq!(labels.last().map(String::as_str), Some("Delete Selection"));
        assert_eq!(history_cursor(&content, cx), cursor_before + 1);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn cross_document_paste_adds_the_clipboard_to_the_active_document(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (_, _, source_pixels) = pixels(&content, cx);
        set_selection(&content, cx, 0.0, 0.0, 1.0, 1.0);
        content.update(cx, |app, cx| {
            app.copy_selection(cx).expect("copy in the source document");
        });
        assert_eq!(
            content.read(cx).clipboard_for_test(),
            Some((1, 1, source_pixels[0..4].to_vec()))
        );

        content.update(cx, |app, cx| {
            app.create_document(4, 4, cx)
                .expect("blank target document");
        });
        window.render_frame(cx);
        let target_layers = content.read(cx).active_core_layer_count_for_test().unwrap();
        content.update(cx, |app, cx| {
            app.paste_selection(cx)
                .expect("the clipboard pastes into the target document");
        });
        assert_eq!(
            content.read(cx).active_core_layer_count_for_test().unwrap(),
            target_layers + 1
        );
        let pasted_id = content
            .read(cx)
            .active_core_layer_ids_for_test()
            .into_iter()
            .find(|id| id.starts_with("native-layer-"))
            .expect("the cross-document paste creates a layer");
        let (width, height, pasted) = content
            .read(cx)
            .core_layer_pixels_for_test(&pasted_id)
            .expect("the pasted layer owns pixels");
        assert_eq!((width, height), (1, 1));
        assert_eq!(pasted, source_pixels[0..4].to_vec());
        let (labels, _) = history_rows(&content, cx);
        assert_eq!(labels.last().map(String::as_str), Some("Paste"));

        content.update(cx, |app, cx| {
            app.activate_document("parity-doc", cx)
                .expect("the source document is still open");
        });
        window.render_frame(cx);
        assert_eq!(
            pixels(&content, cx).2,
            source_pixels,
            "pasting into another document leaves the source untouched"
        );
        let (source_labels, _) = history_rows(&content, cx);
        assert!(
            !source_labels.iter().any(|label| label == "Paste"),
            "the paste belongs to the target document's history"
        );
    })
    .expect("test window remains open");
}
