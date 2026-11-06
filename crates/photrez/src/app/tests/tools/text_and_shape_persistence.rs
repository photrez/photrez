//! Text sessions and the mounted persistence of shape and text layers, including what
//! reaches an exported PNG.

use super::*;

#[gpui_kit::test]
fn text_click_opens_an_editor_and_typing_commits_to_the_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        assert_eq!(
            content.read(cx).text_edit_content_for_test(cx).as_deref(),
            Some(""),
            "the text click opens an empty editor"
        );
        assert!(
            window.try_find("text-editor").is_some(),
            "the in-place editor overlay is mounted"
        );
        assert!(
            window.try_find("text-editor-hint").is_some(),
            "the overlay shows the key legend"
        );

        tab_to_canvas(window, cx);
        window.press("h", cx);
        window.press("i", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).text_edit_content_for_test(cx).as_deref(),
            Some("hi")
        );
        window.press("ctrl-enter", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).text_edit_content_for_test(cx).is_none(),
            "Ctrl+Enter closes the editor"
        );
        let text_id = content
            .read(cx)
            .text_layer_id_for_test()
            .expect("the text layer exists");
        assert_eq!(
            content
                .read(cx)
                .text_layer_content_for_test(&text_id)
                .as_deref(),
            Some("hi"),
            "the typed text is committed to the layer"
        );
        assert!(content
            .read(cx)
            .active_document_save_state_for_test()
            .is_some_and(|(_, _, dirty)| dirty));
    })
    .expect("test window remains open");
}

/// The edit box tracks the session's rasterized preview, so a change to its
/// chrome cannot quietly resize it.
///
/// Bounds only: a mounted test reads geometry, never colour, so the ring's two
/// paints are proven by the capture, not here.
#[gpui_kit::test]
fn text_edit_box_bounds_track_the_session_preview(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);
        let (doc_w, doc_h, _) = pixels(&content, cx);
        drag_doc(window, (doc_w, doc_h), (0.5, 0.5), (0.5, 0.5), cx);
        tab_to_canvas(window, cx);
        window.press("h", cx);
        window.press("i", cx);
        window.render_frame(cx);

        let session = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .text_edit
            .clone()
            .expect("the text session is open");
        let (canvas_bounds, placement) = page(window, (doc_w, doc_h));
        let zoom = placement.2 / doc_w as f32;
        let font_size = session.data.font_size as f32 * zoom;
        let box_bounds = window.find("text-editor-box").bounds();
        assert_eq!(
            f32::from(box_bounds.size.width),
            (session.preview.0 as f32 * zoom).max(24.0),
            "the box width follows the preview width, not its own chrome"
        );
        assert_eq!(
            f32::from(box_bounds.size.height),
            (session.preview.1 as f32 * zoom).max(font_size + 12.0),
            "the box height follows the preview height"
        );
        assert_eq!(
            box_bounds.origin,
            canvas_bounds.origin
                + gpui_kit::point(
                    gpui_kit::px(placement.0 + session.doc_x as f32 * zoom),
                    gpui_kit::px(placement.1 + session.doc_y as f32 * zoom),
                ),
            "the box stays anchored to the layer's document origin"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn text_escape_cancels_the_session_and_removes_the_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let before = content.read(cx).active_core_layer_count_for_test().unwrap();
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        tab_to_canvas(window, cx);
        window.press("x", cx);
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).text_edit_content_for_test(cx).is_none(),
            "Escape closes the editor"
        );
        assert_eq!(
            content.read(cx).active_core_layer_count_for_test().unwrap(),
            before,
            "Escape removes the temporary text layer"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn mounted_shape_saves_and_reloads_as_a_shape_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.1, 0.1), (0.9, 0.9), cx);
        let saved = directory.path().join("shape-saved.ptz");
        content
            .update(cx, |app, cx| app.save_to_path_for_test(&saved, cx))
            .expect("shape document saves");
        let file_io = LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
            .expect("file boundary");
        file_io.approve_path(&saved).expect("saved path approved");
        content
            .update(cx, |app, cx| {
                app.open_startup_document_for_test(&saved, &file_io, cx)
            })
            .expect("saved document reopens");
        let types = content
            .read(cx)
            .active_core_layer_types_for_test()
            .expect("layers exist");
        assert!(
            types
                .iter()
                .any(|kind| matches!(kind, photrez_core::document::content::LayerType::Shape)),
            "the shape layer survives save/reload: {types:?}"
        );
        let _ = source;
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn mounted_text_renders_into_the_exported_png(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(160, 96, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Text;
                state.fg_color = [0, 0, 0, 255];
            });
        });
        window.render_frame(cx);
        drag_doc(window, (160, 96), (12.0, 12.0), (12.0, 12.0), cx);
        tab_to_canvas(window, cx);
        window.press("a", cx);
        window.press("g", cx);
        window.press("ctrl-enter", cx);
        window.render_frame(cx);

        let exported = directory.path().join("text-export.png");
        content
            .update(cx, |app, cx| app.export_to_path_for_test(&exported, cx))
            .expect("text document exports");
        let image = ::image::open(&exported)
            .expect("exported PNG opens")
            .to_rgba8();
        let ink = image
            .pixels()
            .filter(|pixel| pixel[0] < 100 && pixel[1] < 100 && pixel[2] < 100)
            .count();
        assert!(ink > 0, "the exported PNG contains text ink");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn mounted_shape_renders_into_the_exported_png(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(96, 96, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Shape;
                state.fg_color = [0, 0, 0, 255];
                state.shape_kind = crate::state::ShapeTool::Rect;
            });
        });
        window.render_frame(cx);
        drag_doc(window, (96, 96), (16.0, 16.0), (80.0, 80.0), cx);
        let exported = directory.path().join("shape-export.png");
        content
            .update(cx, |app, cx| app.export_to_path_for_test(&exported, cx))
            .expect("shape document exports");
        let image = ::image::open(&exported)
            .expect("exported PNG opens")
            .to_rgba8();
        let ink = image
            .pixels()
            .filter(|pixel| pixel[0] < 100 && pixel[1] < 100 && pixel[2] < 100)
            .count();
        assert!(ink > 100, "the exported PNG contains the shape");
    })
    .expect("test window remains open");
}
