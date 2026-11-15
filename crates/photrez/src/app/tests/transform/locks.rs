//! A locked layer must not move from the canvas, and the arrow keys leave it alone.

use super::*;

/// A locked layer must not move from a canvas drag, including the background
/// position lock. The Move bar fields already refuse a locked layer.
#[gpui_kit::test]
fn a_locked_layer_cannot_be_moved_from_the_canvas(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        new_doc(&content, cx, 400, 300);
        let core_id = selected_core_id(&content, cx);
        content.update(cx, |app, cx| app.toggle_layer_locked(core_id, cx));
        window.render_frame(cx);
        let before = transform(&content, cx);
        let cursor_before = history_cursor(&content, cx);
        drag_doc(window, (400, 300), (200.0, 150.0), (240.0, 180.0), cx);
        assert_eq!(
            transform(&content, cx),
            before,
            "a locked layer must not move from a canvas drag"
        );
        assert_eq!(
            history_cursor(&content, cx),
            cursor_before,
            "a refused drag must not add a history entry"
        );
    })
    .expect("test window remains open");
}

/// The background layer carries a position lock. A canvas drag must not move
/// it, or the Background invariant that the stacking order relies on breaks.
#[gpui_kit::test]
fn a_position_locked_layer_cannot_be_moved_from_the_canvas(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test()
                .update(cx, |state, _| state.active_tool = ToolId::Move);
        });
        assert!(
            content
                .read(cx)
                .selected_layer_for_cursor(cx)
                .expect("the blank document selects its Background")
                .1
                .lock_position
                .unwrap_or(false),
            "the blank document's Background carries the position lock"
        );
        window.render_frame(cx);
        let before = transform(&content, cx);
        let cursor_before = history_cursor(&content, cx);
        drag_doc(window, (400, 300), (200.0, 150.0), (240.0, 180.0), cx);
        assert_eq!(
            transform(&content, cx),
            before,
            "a position-locked layer must not move from a canvas drag"
        );
        assert_eq!(
            history_cursor(&content, cx),
            cursor_before,
            "a refused drag must not add a history entry"
        );
    })
    .expect("test window remains open");
}

/// The arrow-key nudge reads the same transform as the Move bar, so a locked
/// layer must ignore it too.
#[gpui_kit::test]
fn arrow_keys_do_not_move_a_locked_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        new_doc(&content, cx, 400, 300);
        let core_id = selected_core_id(&content, cx);
        content.update(cx, |app, cx| app.toggle_layer_locked(core_id, cx));
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        let before = transform(&content, cx);
        let cursor_before = history_cursor(&content, cx);
        window.press("right", cx);
        window.render_frame(cx);
        assert_eq!(
            transform(&content, cx),
            before,
            "arrow keys must not move a locked layer"
        );
        assert_eq!(
            history_cursor(&content, cx),
            cursor_before,
            "a refused nudge must not add a history entry"
        );
    })
    .expect("test window remains open");
}
