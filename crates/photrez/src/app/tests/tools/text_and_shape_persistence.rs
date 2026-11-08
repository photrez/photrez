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
/// paints are not observed by this test.
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

/// Commit one text layer and open an edit session on it. Returns its id.
fn open_existing_text_session(
    window: &mut gpui_kit::Window,
    content: &Entity<App>,
    cx: &mut gpui_kit::App,
) -> String {
    set_tool(content, cx, ToolId::Text, 1.0);
    window.render_frame(cx);
    let (w, h, _) = pixels(content, cx);
    drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
    tab_to_canvas(window, cx);
    window.press("a", cx);
    content.update(cx, |app, cx| {
        let _ = app.commit_text_edit(cx);
    });
    window.render_frame(cx);
    let layer_id = content
        .read(cx)
        .text_layer_id_for_test()
        .expect("the committed text layer exists");
    let data = content
        .read(cx)
        .text_layer_data_for_test()
        .expect("the layer is text");
    content.update(cx, |app, cx| {
        app.begin_text_edit_existing(layer_id.clone(), 0.0, 0.0, data, cx);
    });
    tab_to_canvas(window, cx);
    layer_id
}

fn click_doc_at(
    window: &mut gpui_kit::Window,
    doc: (u32, u32),
    at: (f32, f32),
    cx: &mut gpui_kit::App,
) {
    let (_, placement) = page(window, doc);
    let local = gpui_kit::point(
        gpui_kit::px(placement.0 + at.0 / doc.0 as f32 * placement.2),
        gpui_kit::px(placement.1 + at.1 / doc.1 as f32 * placement.3),
    );
    window.click_at("canvas-hitbox", local, cx);
    window.render_frame(cx);
}

fn double_click_doc(
    window: &mut gpui_kit::Window,
    doc: (u32, u32),
    at: (f32, f32),
    cx: &mut gpui_kit::App,
) {
    let (bounds, placement) = page(window, doc);
    let position = window_point(bounds, placement, doc, at.0, at.1);
    for click_count in [1usize, 2] {
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                button: gpui_kit::MouseButton::Left,
                position,
                click_count,
                ..Default::default()
            }),
            cx,
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                button: gpui_kit::MouseButton::Left,
                position,
                click_count,
                modifiers: Default::default(),
            }),
            cx,
        );
    }
    window.render_frame(cx);
}

#[gpui_kit::test]
fn double_clicking_a_text_layer_opens_its_editor(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        tab_to_canvas(window, cx);
        window.press("a", cx);
        content.update(cx, |app, cx| {
            let _ = app.commit_text_edit(cx);
        });
        let layer = content
            .read(cx)
            .selected_layer_for_cursor(cx)
            .expect("the text layer is selected")
            .1;
        set_tool(&content, cx, ToolId::Move, 1.0);
        window.render_frame(cx);
        double_click_doc(
            window,
            (w, h),
            (layer.transform.x as f32, layer.transform.y as f32),
            cx,
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_tool,
            ToolId::Text,
            "double-click switches to the Text tool"
        );
        assert_eq!(
            content.read(cx).text_edit_content_for_test(cx).as_deref(),
            Some("a"),
            "double-click opens the layer's editor"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn clicking_inside_the_text_editor_keeps_the_session(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let layer_id = open_existing_text_session(window, &content, cx);
        window.press("b", cx);
        let session = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .text_edit
            .clone()
            .expect("the editor is open");
        let (doc_w, doc_h, _) = pixels(&content, cx);
        let at = (
            (session.doc_x + f64::from(session.preview.0) / 2.0) as f32,
            (session.doc_y + f64::from(session.preview.1) / 2.0) as f32,
        );
        click_doc_at(window, (doc_w, doc_h), at, cx);
        assert_eq!(
            content.read(cx).text_edit_content_for_test(cx).as_deref(),
            Some("ab"),
            "a click inside the edit box keeps the session"
        );
        assert_eq!(
            content
                .read(cx)
                .text_layer_content_for_test(&layer_id)
                .as_deref(),
            Some("a"),
            "the layer is not committed by an inside click"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn ctrl_z_undoes_typing_inside_the_text_editor(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        open_existing_text_session(window, &content, cx);
        window.press("b", cx);
        assert_eq!(
            content.read(cx).text_edit_content_for_test(cx).as_deref(),
            Some("ab")
        );
        window.press("ctrl-z", cx);
        assert_eq!(
            content.read(cx).text_edit_content_for_test(cx).as_deref(),
            Some("a"),
            "Ctrl+Z undoes the typed character inside the editor"
        );
        window.press("ctrl-z", cx);
        assert_eq!(
            content.read(cx).text_edit_content_for_test(cx).as_deref(),
            Some("a"),
            "the undo stack stops at the session's opening text"
        );
        window.press("ctrl-shift-z", cx);
        assert_eq!(
            content.read(cx).text_edit_content_for_test(cx).as_deref(),
            Some("ab"),
            "Ctrl+Shift+Z redoes the character"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn switching_tools_commits_an_open_text_session(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let layer_id = open_existing_text_session(window, &content, cx);
        window.press("b", cx);
        window.click("brush", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).text_edit_content_for_test(cx).is_none(),
            "a tool switch closes the text session"
        );
        assert_eq!(
            content
                .read(cx)
                .text_layer_content_for_test(&layer_id)
                .as_deref(),
            Some("ab"),
            "a tool switch commits the typed text"
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_tool,
            ToolId::Brush,
            "the selected tool still becomes active"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn switching_tabs_commits_an_open_text_session_to_its_document(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(200, 150, cx).expect("first document");
        });
        let first_doc = content
            .read(cx)
            .active_document_id_for_test()
            .expect("first document id");
        content.update(cx, |app, cx| {
            app.create_document(200, 150, cx).expect("second document");
        });
        let second_doc = content
            .read(cx)
            .active_document_id_for_test()
            .expect("second document id");
        content.update(cx, |app, cx| {
            app.activate_document(&first_doc, cx)
                .expect("first document activates");
        });
        window.render_frame(cx);
        let layer_id = open_existing_text_session(window, &content, cx);
        window.press("b", cx);
        content.update(cx, |app, cx| {
            app.activate_document(&second_doc, cx)
                .expect("second document activates");
        });
        window.render_frame(cx);
        assert!(
            content.read(cx).text_edit_content_for_test(cx).is_none(),
            "a tab switch closes the text session"
        );
        let text = content
            .read(cx)
            .sessions
            .get(&first_doc)
            .and_then(|session| session.loaded_document().content_snapshot())
            .and_then(|content| content.layers.iter().find(|layer| layer.id == layer_id))
            .and_then(|layer| layer.text_data.as_ref())
            .map(|data| data.content.clone());
        assert_eq!(
            text.as_deref(),
            Some("ab"),
            "the text is committed to the document that owns the layer"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn undo_while_editing_text_cancels_the_session_without_stepping_history(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let layer_id = open_existing_text_session(window, &content, cx);
        window.press("b", cx);
        let cursor_before = content
            .read(cx)
            .active_core_history_cursor_for_test()
            .expect("the fixture has history");
        content.update(cx, |app, cx| {
            app.undo_document_action(&crate::app::history::UndoDocument, cx);
        });
        window.render_frame(cx);
        assert!(
            content.read(cx).text_edit_content_for_test(cx).is_none(),
            "undo closes the text session"
        );
        assert_eq!(
            content
                .read(cx)
                .active_core_history_cursor_for_test()
                .expect("the fixture has history"),
            cursor_before,
            "undo during a text edit does not step document history"
        );
        assert_eq!(
            content
                .read(cx)
                .text_layer_content_for_test(&layer_id)
                .as_deref(),
            Some("a"),
            "cancelling an existing text session keeps the original text"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn deleting_the_edited_text_layer_closes_the_session(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        open_existing_text_session(window, &content, cx);
        window.press("b", cx);
        let before = content
            .read(cx)
            .active_core_layer_count_for_test()
            .expect("layer count");
        content.update(cx, |app, cx| app.delete_selected_layer(cx));
        window.render_frame(cx);
        assert!(
            content.read(cx).text_edit_content_for_test(cx).is_none(),
            "deleting the edited layer closes the text session"
        );
        assert_eq!(
            content
                .read(cx)
                .active_core_layer_count_for_test()
                .expect("layer count"),
            before - 1,
            "the layer is gone"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn cancelling_an_existing_text_session_keeps_the_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let layer_id = open_existing_text_session(window, &content, cx);
        window.press("b", cx);
        let before = content
            .read(cx)
            .active_core_layer_count_for_test()
            .expect("layer count");
        content.update(cx, |app, cx| {
            let _ = app.cancel_text_edit(cx);
        });
        window.render_frame(cx);
        assert!(
            content.read(cx).text_edit_content_for_test(cx).is_none(),
            "cancel closes the session"
        );
        assert_eq!(
            content
                .read(cx)
                .active_core_layer_count_for_test()
                .expect("layer count"),
            before,
            "cancelling an existing text session keeps its layer"
        );
        assert_eq!(
            content
                .read(cx)
                .text_layer_content_for_test(&layer_id)
                .as_deref(),
            Some("a"),
            "the original text survives cancel"
        );
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
