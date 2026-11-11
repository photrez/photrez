//! Transformable-state gating for the Move frame.

use super::canvas_input::set_tool;
use super::*;
use crate::state::ToolId;
use gpui_kit::test::TestWindowExt as _;

/// A locked Background is not transformable, so Move paints no bounding box.
/// Unlocking it restores the frame, so the rule follows the locks, not the
/// background role.
#[gpui_kit::test]
fn a_locked_background_paints_no_transform_frame(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Move, 1.0);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            assert!(
                app.transform_frame_for_test(cx).is_none(),
                "a locked Background paints no bounding box"
            );
            assert!(!app.canvas_overlay(cx).transform_controls);
        });
        super::canvas_input::unlock_background_transform(&content, cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            assert!(
                app.transform_frame_for_test(cx).is_some(),
                "unlocking the Background restores its frame"
            );
        });
    })
    .expect("test window remains open");
}

/// A project loaded from an older file can mark its bottom layer as the
/// Background without storing per-axis lock fields. The role still locks it,
/// so selecting that row paints no frame.
#[gpui_kit::test]
fn a_loaded_background_with_absent_lock_fields_paints_no_transform_frame(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let path = directory.path().join("background.ptz");
    std::fs::write(
        &path,
        super::commands::multi_layer_archive_with_background(),
    )
    .expect("background fixture is written");
    let file_io =
        super::LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
            .expect("file boundary opens");
    file_io
        .approve_path(&path)
        .expect("background fixture is approved");
    cx.update_window(window.into(), |_, window, cx| {
        content
            .update(cx, |app, cx| {
                app.open_startup_document_for_test(&path, &file_io, cx)
            })
            .expect("the background fixture opens");
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                let row = state
                    .layers
                    .iter()
                    .find(|row| row.core_id.as_deref() == Some("bottom"))
                    .expect("the Background row projects");
                state.selected_layer = row.id;
                state.selected_layers = vec![row.id];
                state.active_tool = ToolId::Move;
                state.show_transform_controls = true;
            });
        });
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            assert!(
                app.transform_frame_for_test(cx).is_none(),
                "a Background locked by role paints no bounding box"
            );
            assert!(!app.canvas_overlay(cx).transform_controls);
        });
    })
    .expect("test window remains open");
}
