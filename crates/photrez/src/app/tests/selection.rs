use super::canvas_input::{history_cursor, page, pixels, set_tool, tab_to_canvas, window_point};
use super::*;
use crate::state::ToolId;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput};
use photrez_core::document::content::{SelectionShape, SelectionState};

mod commands;
mod rotate;

pub(super) fn dispatch(
    window: &mut gpui_kit::Window,
    event: PlatformInput,
    cx: &mut gpui_kit::App,
) {
    let _ = window.dispatch_event(event, cx);
    window.render_frame(cx);
}

pub(super) fn set_selection(
    content: &Entity<App>,
    cx: &mut gpui_kit::App,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) {
    content.update(cx, |app, cx| {
        app.set_core_selection_for_test(
            SelectionState {
                x,
                y,
                width,
                height,
                angle: 0.0,
                shape: Some(SelectionShape::Rect),
                inverted: None,
            },
            cx,
        )
        .expect("core accepts the selection");
    });
}

pub(super) fn history_rows(
    content: &Entity<App>,
    cx: &mut gpui_kit::App,
) -> (Vec<String>, Option<usize>) {
    let state = content.read(cx).state_for_test().read(cx);
    (
        state.history.iter().map(|row| row.label.clone()).collect(),
        state.history.iter().position(|row| row.current),
    )
}

pub(super) fn status_error(content: &Entity<App>, cx: &mut gpui_kit::App) -> Option<String> {
    content
        .read(cx)
        .state_for_test()
        .read(cx)
        .status_error
        .clone()
}

#[gpui_kit::test]
fn selection_drag_previews_then_commits_to_core(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let doc = (w, h);
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 0.1, 0.1);
        let moved = window_point(bounds, placement, doc, 0.9, 0.9);
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        let preview = content
            .read(cx)
            .selection_drag_for_test(cx)
            .expect("the drag publishes a marquee preview");
        assert!(preview[2] > 0. && preview[3] > 0.);
        assert!(
            content.read(cx).active_core_selection_for_test().is_none(),
            "the core selection commits on pointer-up"
        );

        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        assert!(
            content.read(cx).selection_drag_for_test(cx).is_none(),
            "the preview clears when the selection commits"
        );
        let selection = content
            .read(cx)
            .active_core_selection_for_test()
            .expect("pointer-up commits the core selection");
        assert!(selection.width >= 1.0 && selection.height >= 1.0);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn shift_constrains_and_alt_centres_a_pixel_selection(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let doc = (w, h);
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 0.1, 0.1);
        let end = window_point(bounds, placement, doc, 0.5, 0.9);
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                modifiers: gpui_kit::Modifiers {
                    shift: true,
                    ..Default::default()
                },
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: end,
                ..Default::default()
            }),
            cx,
        );
        let square = content
            .read(cx)
            .selection_drag_for_test(cx)
            .expect("preview exists");
        assert!(
            (square[2] - square[3]).abs() < 1e-3,
            "Shift must produce a square: {square:?}"
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                ..Default::default()
            }),
            cx,
        );

        let centre = window_point(bounds, placement, doc, 0.5, 0.5);
        let edge = window_point(bounds, placement, doc, 1.5, 0.9);
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: centre,
                modifiers: gpui_kit::Modifiers {
                    alt: true,
                    ..Default::default()
                },
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: edge,
                ..Default::default()
            }),
            cx,
        );
        let centred = content
            .read(cx)
            .selection_drag_for_test(cx)
            .expect("preview exists");
        assert!(
            centred[0] < 0.5 && centred[1] < 0.5,
            "Alt anchors both axes at the centre: {centred:?}"
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: edge,
                ..Default::default()
            }),
            cx,
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn selection_shortcuts_toggle_core_selection_without_history(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        let cursor_before = history_cursor(&content, cx);

        window.press("ctrl-a", cx);
        window.render_frame(cx);
        let selection = content
            .read(cx)
            .active_core_selection_for_test()
            .expect("Ctrl+A selects the whole canvas");
        assert_eq!(
            (
                selection.x,
                selection.y,
                selection.width,
                selection.height,
                selection.inverted
            ),
            (0.0, 0.0, 2.0, 1.0, None)
        );

        window.press("ctrl-shift-i", cx);
        window.render_frame(cx);
        let selection = content
            .read(cx)
            .active_core_selection_for_test()
            .expect("the selection survives the invert");
        assert_eq!(selection.inverted, Some(true));

        window.press("ctrl-d", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).active_core_selection_for_test().is_none(),
            "Ctrl+D clears the selection"
        );

        // Inverting with nothing selected falls back to select-all, matching
        // the core command's host contract.
        window.press("ctrl-shift-i", cx);
        window.render_frame(cx);
        let selection = content
            .read(cx)
            .active_core_selection_for_test()
            .expect("invert without a selection selects all");
        assert_eq!((selection.width, selection.height), (2.0, 1.0));
        assert_eq!(selection.inverted, None);

        assert_eq!(
            history_cursor(&content, cx),
            cursor_before,
            "selection changes are not history entries"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn ctrl_t_toggles_the_selection_transform_chrome(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        window.press("ctrl-t", cx);
        window.render_frame(cx);
        assert!(
            !content.read(cx).selection_edit_mode_for_test(),
            "Ctrl+T without a selection does nothing"
        );

        set_selection(&content, cx, 0.0, 0.0, 1.0, 1.0);
        window.render_frame(cx);
        window.press("ctrl-t", cx);
        window.render_frame(cx);
        assert!(content.read(cx).selection_edit_mode_for_test());
        window.press("ctrl-t", cx);
        window.render_frame(cx);
        assert!(
            !content.read(cx).selection_edit_mode_for_test(),
            "a second Ctrl+T closes the transform chrome"
        );
        window.press("ctrl-t", cx);
        window.render_frame(cx);
        assert!(content.read(cx).selection_edit_mode_for_test());

        window.press("escape", cx);
        window.render_frame(cx);
        assert!(
            !content.read(cx).selection_edit_mode_for_test(),
            "Escape clears the transform chrome"
        );
        window.press("ctrl-t", cx);
        window.render_frame(cx);
        assert!(content.read(cx).selection_edit_mode_for_test());
        window.press("ctrl-d", cx);
        window.render_frame(cx);
        assert!(
            !content.read(cx).selection_edit_mode_for_test(),
            "deselect clears the transform chrome"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn marquee_overlay_paints_live_and_committed_selection(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let doc = (w, h);
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 0.2, 0.2);
        let moved = window_point(bounds, placement, doc, 0.8, 0.7);
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        let preview = content
            .read(cx)
            .selection_drag_for_test(cx)
            .expect("the drag publishes a preview");
        let overlay = content.update(cx, |app, cx| app.canvas_overlay(cx));
        let live = overlay
            .selection
            .expect("the live marquee reaches the overlay");
        let (x, y, width, height, ellipse) = (live.x, live.y, live.w, live.h, live.ellipse);
        assert_eq!(
            (x, y, width, height, ellipse),
            (preview[0], preview[1], preview[2], preview[3], false)
        );

        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        let committed = content
            .read(cx)
            .active_core_selection_for_test()
            .expect("pointer-up commits the core selection");
        let overlay = content.update(cx, |app, cx| app.canvas_overlay(cx));
        let painted = overlay
            .selection
            .expect("the committed selection keeps painting");
        let (x, y, width, height) = (painted.x, painted.y, painted.w, painted.h);
        assert_eq!(
            (x, y, width, height),
            (committed.x, committed.y, committed.width, committed.height)
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn canvas_click_selection_covers_the_clicked_point(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let doc = (w, h);
        let (bounds, placement) = page(window, doc);
        let point = window_point(bounds, placement, doc, 0.4, 0.6);
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: point,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: point,
                ..Default::default()
            }),
            cx,
        );
        let overlay = content.update(cx, |app, cx| app.canvas_overlay(cx));
        match content.read(cx).active_core_selection_for_test() {
            Some(selection) => {
                let click_x = 0.4 * f64::from(doc.0);
                let click_y = 0.6 * f64::from(doc.1);
                assert!(
                    click_x >= selection.x
                        && click_x <= selection.x + selection.width
                        && click_y >= selection.y
                        && click_y <= selection.y + selection.height,
                    "a click-created selection must cover the clicked point: \
                     selection {selection:?} does not contain ({click_x}, {click_y})"
                );
            }
            None => assert!(
                overlay.selection.is_none(),
                "no committed selection must not paint a marquee"
            ),
        }
    })
    .expect("test window remains open");
}

/// The selection bar's X/Y/W/H/R fields read the committed
/// selection, their commits rewrite one dimension through core history, and
/// without a selection the bar stays disabled.
#[gpui_kit::test]
fn selection_bar_fields_read_and_rewrite_the_committed_selection(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    // The bar needs the full layout width: the wide dock (598 px) leaves 886 px
    // and clips the R field at 1536 px.
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1920.0);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 200, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);

        // Disabled state: the static field accepts no edit and nothing
        // starts a num-field session.
        window.click("sel-w", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).committed_selection().is_none(),
            "the disabled field must not create a selection"
        );
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .num_field
                .is_none(),
            "the disabled field must not start an edit session"
        );

        set_selection(&content, cx, 40.0, 30.0, 120.0, 80.0);
        window.render_frame(cx);

        // W rewrites only the width through core history.
        window.click("sel-w", cx);
        window.render_frame(cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .num_field
                .is_some(),
            "clicking the W field starts an edit session"
        );
        window.press("2", cx);
        window.press("0", cx);
        window.press("0", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        let selection = content
            .read(cx)
            .committed_selection()
            .expect("the selection survives the field edit");
        assert_eq!(
            selection.width,
            200.0,
            "the W field commits its parsed value (status {:?}, field {:?})",
            status_error(&content, cx),
            content.read(cx).state_for_test().read(cx).num_field
        );
        assert_eq!(
            (selection.x, selection.y, selection.height),
            (40.0, 30.0, 80.0),
            "the other dimensions survive the rewrite"
        );

        // X moves the box.
        window.click("sel-x", cx);
        window.press("6", cx);
        window.press("0", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).committed_selection().expect("selection").x,
            60.0,
            "the X field moves the selection"
        );

        // The width field's `n > 0` guard: a non-positive edit is ignored.
        window.click("sel-w", cx);
        window.press("0", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            content
                .read(cx)
                .committed_selection()
                .expect("selection")
                .width,
            200.0,
            "a non-positive width edit is ignored"
        );

        // R rotates the selection.
        window.click("sel-r", cx);
        window.press("3", cx);
        window.press("0", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            content
                .read(cx)
                .committed_selection()
                .expect("selection")
                .angle,
            30.0,
            "the R field commits the angle"
        );
    })
    .expect("test window remains open");
}
