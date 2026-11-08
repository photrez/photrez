use super::*;
use crate::display::canvas_bridge::fit_placement;
use crate::input_policy::PendingGesture;
use crate::state::{SelectionShape, ToolId};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    point, px, Bounds, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    PlatformInput, Point,
};
use photrez_core::document::content::LayerType;

mod brush_input;
mod escape_and_preview;
mod gesture_cancellation;
mod history_and_undo;
mod history_panel;
mod pointer_capture;
mod tool_and_layer_coverage;

pub(super) fn set_tool(content: &Entity<App>, cx: &mut gpui_kit::App, tool: ToolId, size: f64) {
    content.update(cx, |app, cx| {
        app.state_for_test().update(cx, |state, _| {
            state.active_tool = tool;
            state.brush_size = size;
            state.fg_color = [0, 0, 0, 255];
            state.selection_shape = SelectionShape::Rect;
        });
    });
}

pub(super) fn pixels(content: &Entity<App>, cx: &mut gpui_kit::App) -> (u32, u32, Vec<u8>) {
    content
        .read(cx)
        .active_core_pixels_for_test()
        .expect("fixture has a raster layer")
}

/// Remove the transform locks from the blank document's Background so a test
/// can move it. The real Background carries `lock_position`, and the Move tool
/// correctly refuses to drag it.
pub(super) fn unlock_background_transform(content: &Entity<App>, cx: &mut gpui_kit::App) {
    let target = {
        let app = content.read(cx);
        let Some(doc_id) = app.active_document.as_ref().map(|doc| doc.doc_id.clone()) else {
            return;
        };
        let Some(layer) = app
            .sessions
            .get(&doc_id)
            .and_then(|session| session.loaded_document().content_snapshot())
            .and_then(|content| {
                content
                    .layers
                    .iter()
                    .find(|layer| layer.is_background == Some(true))
            })
        else {
            return;
        };
        (
            layer.id.clone(),
            layer.lock_position.unwrap_or(false),
            layer.lock_rotation.unwrap_or(false),
        )
    };
    content.update(cx, |app, cx| {
        if target.1 {
            app.toggle_layer_lock(
                target.0.clone(),
                photrez_core::protocol::LockKind::Position,
                cx,
            );
        }
        if target.2 {
            app.toggle_layer_lock(target.0, photrez_core::protocol::LockKind::Rotation, cx);
        }
    });
}

pub(super) fn page(
    window: &mut gpui_kit::Window,
    doc: (u32, u32),
) -> (Bounds<Pixels>, (f32, f32, f32, f32)) {
    let bounds = window.find("canvas-hitbox").bounds();
    let page = fit_placement(
        bounds.size.width.into(),
        bounds.size.height.into(),
        doc.0 as f32,
        doc.1 as f32,
    )
    .expect("the canvas has a page placement");
    (bounds, page)
}

pub(super) fn window_point(
    bounds: Bounds<Pixels>,
    page: (f32, f32, f32, f32),
    doc: (u32, u32),
    x: f32,
    y: f32,
) -> Point<Pixels> {
    let (page_x, page_y, page_w, page_h) = page;
    let local_x = page_x + x / doc.0 as f32 * page_w;
    let local_y = page_y + y / doc.1 as f32 * page_h;
    bounds.origin + point(px(local_x), px(local_y))
}

fn local_point(page: (f32, f32, f32, f32), doc: (u32, u32), x: f32, y: f32) -> Point<Pixels> {
    let (page_x, page_y, page_w, page_h) = page;
    point(
        px(page_x + x / doc.0 as f32 * page_w),
        px(page_y + y / doc.1 as f32 * page_h),
    )
}

pub(super) fn drag_doc(
    window: &mut gpui_kit::Window,
    doc: (u32, u32),
    from: (f32, f32),
    to: (f32, f32),
    cx: &mut gpui_kit::App,
) {
    let (bounds, page) = page(window, doc);
    window.drag(
        window_point(bounds, page, doc, from.0, from.1),
        window_point(bounds, page, doc, to.0, to.1),
        cx,
    );
    window.render_frame(cx);
}

fn click_doc(
    window: &mut gpui_kit::Window,
    doc: (u32, u32),
    at: (f32, f32),
    cx: &mut gpui_kit::App,
) {
    let (_, page) = page(window, doc);
    window.click_at("canvas-hitbox", local_point(page, doc, at.0, at.1), cx);
    window.render_frame(cx);
}

fn history_version(content: &Entity<App>, cx: &mut gpui_kit::App) -> u64 {
    content
        .read(cx)
        .active_core_history_version_for_test()
        .expect("fixture has a history version")
}

fn dispatch_pointer_event(
    window: &mut gpui_kit::Window,
    event: PlatformInput,
    cx: &mut gpui_kit::App,
) {
    let _ = window.dispatch_event(event, cx);
}

pub(super) fn tab_to_canvas(window: &mut gpui_kit::Window, cx: &mut gpui_kit::App) {
    window.focus_next(cx);
    window.render_frame(cx);
    for _ in 0..64 {
        window.press("tab", cx);
        if window.find("canvas-hitbox").focused() == Some(true) {
            return;
        }
    }
    panic!("Tab traversal did not focus the canvas");
}

fn assert_pending_escape_left_core_unchanged(
    content: &Entity<App>,
    cx: &mut gpui_kit::App,
    before_pixels: &[u8],
    before_layers: usize,
    before_transform: Option<(f64, f64)>,
    before_history: u64,
) {
    assert_eq!(pixels(content, cx).2.as_slice(), before_pixels);
    assert_eq!(
        content
            .read(cx)
            .active_core_layer_count_for_test()
            .expect("fixture has a layer count"),
        before_layers
    );
    assert_eq!(
        content.read(cx).active_core_transform_for_test(),
        before_transform
    );
    assert_eq!(history_version(content, cx), before_history);
    assert!(content.read(cx).active_core_selection_for_test().is_none());
    content.update(cx, |app, cx| {
        assert!(app.state_for_test().read(cx).crop_rect.is_none());
    });
}

pub(super) fn mount_archive_app(
    cx: &mut gpui_kit::TestAppContext,
    directory: &TestDirectory,
    bytes: Vec<u8>,
) -> (
    gpui_kit::WindowHandle<gpui_kit::base::Root>,
    Entity<App>,
    std::path::PathBuf,
) {
    let source = directory.path().join("archive.ptz");
    std::fs::write(&source, bytes).expect("archive fixture is written");
    let file_io = Arc::new(
        LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
            .expect("local file boundary opens"),
    );
    file_io.approve_path(&source).expect("fixture is approved");
    let (window, content) = mount_app(cx, Some(source.clone()), Some(file_io));
    (window, content, source)
}

pub(super) fn layer_rows(
    content: &Entity<App>,
    cx: &mut gpui_kit::App,
) -> (Vec<crate::state::LayerRow>, u64) {
    let state = content.read(cx).state_for_test().read(cx);
    (state.layers.clone(), state.selected_layer)
}

pub(super) fn select_layer(content: &Entity<App>, cx: &mut gpui_kit::App, row_id: u64) {
    content.update(cx, |app, cx| {
        app.state_for_test()
            .update(cx, |state, _| state.selected_layer = row_id);
    });
}

#[gpui_kit::test]
fn brush_drag_mutates_core_pixels_and_history(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        let doc = {
            let (w, h, _) = pixels(&content, cx);
            (w, h)
        };
        let (_, _, before) = pixels(&content, cx);
        let version_before = history_version(&content, cx);
        drag_doc(window, doc, (0.5, 0.5), (1.5, 1.5), cx);
        let (_, _, after) = pixels(&content, cx);
        assert_ne!(after, before, "the drag must change canonical pixels");
        assert!(history_version(&content, cx) > version_before);
        content.update(cx, |app, _| {
            assert_eq!(
                app.active_document_save_state_for_test(),
                Some((1, 0, true))
            );
        });
        assert_eq!(
            content.read(cx).input_phase_for_test(),
            crate::input_policy::GesturePhase::Idle
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn eraser_drag_changes_pixels_after_brush(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 8.0);
        window.render_frame(cx);
        let doc = {
            let (w, h, _) = pixels(&content, cx);
            (w, h)
        };
        drag_doc(window, doc, (0.5, 0.5), (2.0, 2.0), cx);
        let (_, _, painted) = pixels(&content, cx);
        set_tool(&content, cx, ToolId::Eraser, 8.0);
        drag_doc(window, doc, (0.5, 0.5), (2.0, 2.0), cx);
        let (_, _, erased) = pixels(&content, cx);
        assert_ne!(erased, painted, "the eraser must change the painted pixels");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn paint_bucket_click_fills_with_the_foreground_color(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::PaintBucket, 1.0);
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.fg_color = [0, 255, 0, 255];
            });
        });
        window.render_frame(cx);
        let (w, h, before) = pixels(&content, cx);
        click_doc(window, (w, h), (0.5, 0.5), cx);
        let (_, _, after) = pixels(&content, cx);
        assert_ne!(after, before);
        assert!(
            after.chunks_exact(4).any(|p| p == [0, 255, 0, 255]),
            "the fill must write the foreground colour"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn gradient_drag_changes_pixels(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Gradient, 1.0);
        window.render_frame(cx);
        let (w, h, before) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (w as f32 - 1.5, 0.5), cx);
        let (_, _, after) = pixels(&content, cx);
        assert_ne!(after, before, "the gradient must write pixels");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn selection_drag_sets_core_selection(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (2.5, 2.5), cx);
        let selection = content
            .read(cx)
            .active_core_selection_for_test()
            .expect("the drag must set core selection");
        assert!(selection.width >= 1.0 && selection.height >= 1.0);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn move_drag_translates_the_core_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        // A 2x1 page at the test window's zoom is smaller than the 16px handle
        // box, so its centre reports a corner handle. Move needs a layer whose
        // interior is clear of the handles.
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
        });
        unlock_background_transform(&content, cx);
        set_tool(&content, cx, ToolId::Move, 1.0);
        window.render_frame(cx);
        let before = content
            .read(cx)
            .active_core_transform_for_test()
            .expect("raster transform");
        drag_doc(window, (400, 300), (200.0, 150.0), (260.0, 190.0), cx);
        let after = content
            .read(cx)
            .active_core_transform_for_test()
            .expect("raster transform");
        assert_ne!(after, before, "the move drag must update the transform");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn shape_drag_adds_a_shape_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let before = content.read(cx).active_core_layer_count_for_test().unwrap();
        drag_doc(window, (w, h), (0.5, 0.5), (2.5, 2.5), cx);
        let status = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .status_error
            .clone();
        assert!(status.is_none(), "shape status: {status:?}");
        let types = content.read(cx).active_core_layer_types_for_test().unwrap();
        assert_eq!(types.len(), before + 1);
        assert!(types.iter().any(|t| matches!(t, LayerType::Shape)));
        let origin = content
            .read(cx)
            .active_core_layer_origin_for_test(LayerType::Shape)
            .expect("shape layer has an origin");
        assert!(
            (origin.0 - 0.5).abs() < 0.01 && origin.1.abs() < 0.01,
            "the shape must land at the drag origin (the 2x1 fixture clamps y), got {origin:?}"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn text_click_adds_a_text_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let before = content.read(cx).active_core_layer_count_for_test().unwrap();
        click_doc(window, (w, h), (0.5, 0.5), cx);
        let status = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .status_error
            .clone();
        assert!(status.is_none(), "text status: {status:?}");
        let types = content.read(cx).active_core_layer_types_for_test().unwrap();
        assert_eq!(types.len(), before + 1);
        assert!(types.iter().any(|t| matches!(t, LayerType::Text)));
        let origin = content
            .read(cx)
            .active_core_layer_origin_for_test(LayerType::Text)
            .expect("text layer has an origin");
        assert!(
            (origin.0 - 0.5).abs() < 0.01 && origin.1.abs() < 0.01,
            "the text layer must land at the click point (the 2x1 fixture clamps y), got {origin:?}"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_drag_sets_the_pending_crop_rect(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let before = content.read(cx).active_core_layer_count_for_test().unwrap();
        drag_doc(window, (w, h), (0.5, 0.5), (2.5, 2.5), cx);
        content.update(cx, |app, cx| {
            let rect = app.state_for_test().read(cx).crop_rect;
            assert!(rect.is_some_and(|r| r[2] >= 1.0 && r[3] >= 1.0));
        });
        assert_eq!(
            content.read(cx).active_core_layer_count_for_test().unwrap(),
            before,
            "the crop pointer only sets the pending rectangle"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn eyedropper_samples_the_non_first_pixel_without_mutation(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Eyedropper, 1.0);
        window.render_frame(cx);
        let (w, h, before) = pixels(&content, cx);
        assert_eq!((w, h), (2, 1), "fixture must contain two document pixels");
        let first: [u8; 4] = before[0..4].try_into().expect("first RGBA pixel");
        let expected: [u8; 4] = before[4..8].try_into().expect("second RGBA pixel");
        assert_ne!(first, expected, "fixture pixels must be distinct");
        let version_before = history_version(&content, cx);
        click_doc(window, (w, h), (1.5, 0.5), cx);
        content.update(cx, |app, cx| {
            assert_eq!(app.state_for_test().read(cx).fg_color, expected);
        });
        assert_eq!(
            pixels(&content, cx).2,
            before,
            "sampling must not mutate pixels"
        );
        assert_eq!(history_version(&content, cx), version_before);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn zero_size_selection_and_crop_normalize_to_one_pixel(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let (w, h, _) = pixels(&content, cx);
        let doc = (w, h);

        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        click_doc(window, doc, (0.5, 0.5), cx);
        let selection = content
            .read(cx)
            .active_core_selection_for_test()
            .expect("a zero-size click still sets a selection");
        assert!(
            selection.width >= 1.0 && selection.height >= 1.0,
            "zero-size selection must normalize to one pixel"
        );

        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        click_doc(window, doc, (0.5, 0.5), cx);
        content.update(cx, |app, cx| {
            let rect = app
                .state_for_test()
                .read(cx)
                .crop_rect
                .expect("a zero-size click still sets a crop rect");
            assert!(
                rect[2] >= 1.0 && rect[3] >= 1.0,
                "zero-size crop must normalize to one pixel"
            );
        });
        assert_eq!(
            content
                .read(cx)
                .active_core_layer_count_for_test()
                .expect("fixture has a layer count"),
            1,
            "the crop pointer only sets the pending rectangle"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn click_outside_the_page_does_not_mutate(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        let (w, h, before) = pixels(&content, cx);
        let doc = (w, h);
        let history_before = history_version(&content, cx);
        let (_, placement) = page(window, doc);

        // Above the page, still inside the hitbox for the wide test fixture.
        window.click_at("canvas-hitbox", local_point(placement, doc, 0.5, -0.1), cx);
        window.render_frame(cx);
        assert_eq!(
            pixels(&content, cx).2,
            before,
            "a click outside the page must not paint"
        );
        assert_eq!(
            history_version(&content, cx),
            history_before,
            "a click outside the page must not add history"
        );
        assert_eq!(
            content.read(cx).input_phase_for_test(),
            crate::input_policy::GesturePhase::Idle
        );
    })
    .expect("test window remains open");
}

pub(super) fn history_cursor(content: &Entity<App>, cx: &mut gpui_kit::App) -> usize {
    content
        .read(cx)
        .active_core_history_cursor_for_test()
        .expect("fixture has a history cursor")
}

#[gpui_kit::test]
fn layer_shortcuts_duplicate_merge_and_flatten(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        window.press("ctrl-j", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_core_layer_ids_for_test().len(),
            3,
            "Ctrl+J duplicates the selected layer"
        );
        window.press("ctrl-z", cx);
        window.render_frame(cx);
        assert_eq!(content.read(cx).active_core_layer_ids_for_test().len(), 2);

        window.press("ctrl-e", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_core_layer_ids_for_test().len(),
            1,
            "Ctrl+E merges the selected layer down"
        );
        window.press("ctrl-z", cx);
        window.render_frame(cx);

        window.press("ctrl-shift-e", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_core_layer_ids_for_test().len(),
            1,
            "Ctrl+Shift+E flattens the document"
        );
        window.press("ctrl-z", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_core_layer_ids_for_test(),
            vec!["top", "bottom"],
            "undo restores the flattened document"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn save_after_move_persists_the_live_transform(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, source) = mount_fixture_app(cx, &directory);
    let moved = cx
        .update_window(window.into(), |_, window, cx| {
            content.update(cx, |app, cx| {
                app.create_document(400, 300, cx).expect("blank document");
            });
            unlock_background_transform(&content, cx);
            set_tool(&content, cx, ToolId::Move, 2.0);
            window.render_frame(cx);
            let before = content
                .read(cx)
                .active_core_transform_for_test()
                .expect("fixture has a transform");
            drag_doc(window, (400, 300), (200.0, 150.0), (240.0, 150.0), cx);
            let moved = content
                .read(cx)
                .active_core_transform_for_test()
                .expect("fixture has a transform");
            assert_ne!(before, moved, "the move must change the transform");
            content
                .update(cx, |app, cx| app.save_to_path_for_test(&source, cx))
                .expect("the moved document saves");
            content.update(cx, |app, _cx| {
                let (saved, current, dirty) = app
                    .active_document_save_state_for_test()
                    .expect("the moved document has a save state");
                assert_eq!(
                    (saved, dirty),
                    (current, false),
                    "a metadata edit must save and clear dirty"
                );
            });
            moved
        })
        .expect("test window remains open");

    let file_io = LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
        .expect("local file boundary opens");
    file_io.approve_path(&source).expect("saved path approved");
    let loaded = crate::services::document::load_document_candidate(&file_io, &source)
        .expect("the saved document reloads");
    let saved = loaded.content_snapshot().expect("saved content is present");
    assert_eq!(
        (saved.layers[0].transform.x, saved.layers[0].transform.y),
        moved,
        "the saved file must carry the live transform"
    );
}

/// The Move tool clears its transform selection on an empty click, but the
/// paint target survives. A brush stroke must still land on the last active
/// layer.
#[gpui_kit::test]
fn paint_still_targets_the_last_layer_after_a_move_deselect(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Move, 2.0);
        window.render_frame(cx);
        let before = pixels(&content, cx).2;
        click_doc(window, (400, 300), (200.0, 150.0), cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .selected_layers
                .is_empty(),
            "the empty click clears the transform selection"
        );

        set_tool(&content, cx, ToolId::Brush, 8.0);
        window.render_frame(cx);
        let cursor_before = history_cursor(&content, cx);
        drag_doc(window, (400, 300), (200.0, 150.0), (240.0, 180.0), cx);
        assert_ne!(
            pixels(&content, cx).2,
            before,
            "the brush still paints on the last active layer"
        );
        assert_eq!(
            history_cursor(&content, cx),
            cursor_before + 1,
            "the stroke commits one history entry"
        );
    })
    .expect("test window remains open");
}
