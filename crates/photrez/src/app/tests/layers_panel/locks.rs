//! The lock controls: the background row warns, the toasts cap at three, and the header lock drives core.

use super::*;

#[gpui_kit::test]
fn background_lock_click_shows_the_warn_toast(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with_background(),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("toast-1").is_none(),
            "the live host is empty at rest"
        );
        let (rows, _) = super::canvas_input::layer_rows(&content, cx);
        let bottom = rows
            .iter()
            .find(|row| row.core_id.as_deref() == Some("bottom"))
            .expect("bottom row")
            .id;
        window.click(format("layer-lock-{bottom}"), cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.toasts.len(), 1, "the guarded click posts one toast");
            assert_eq!(state.toasts[0].message, "Rename the layer to unlock");
            assert_eq!(state.toasts[0].severity, crate::state::ToastSeverity::Warn);
        }
        assert!(
            window.try_find("toast-1").is_some(),
            "the host paints the live queue"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn live_toasts_cap_at_three_and_dismiss_by_id(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            for index in 0..4 {
                app.show_toast(
                    format("toast {index}"),
                    crate::state::ToastSeverity::Info,
                    cx,
                );
            }
        });
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.toasts.len(), 3, "the queue keeps the last three");
            assert_eq!(
                state.toasts[0].message, "toast 1",
                "the oldest toast is dropped"
            );
            assert_eq!(state.next_toast_id, 5);
        }
        assert!(
            window.try_find("toast-1").is_none(),
            "the dropped row is gone"
        );
        assert!(
            window.try_find("toast-4").is_some(),
            "the newest row paints"
        );

        content.update(cx, |app, cx| app.dismiss_toast(3, cx));
        window.render_frame(cx);
        assert!(
            window.try_find("toast-3").is_none(),
            "dismiss removes the row by id"
        );
        assert!(window.try_find("toast-2").is_some(), "siblings survive");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn header_lock_toggles_drive_core_and_disable_the_partial_locks(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (rows, _) = super::canvas_input::layer_rows(&content, cx);
        let top = rows
            .iter()
            .find(|row| row.core_id.as_deref() == Some("top"))
            .expect("top row")
            .id;
        select_layer(&content, cx, top);
        window.render_frame(cx);

        window.click("layer-lock-header-base", cx);
        window.render_frame(cx);
        assert!(
            flags(&content, cx)[0].locked,
            "the header's base toggle locks the selected layer"
        );

        window.click("layer-lock-header-transparency", cx);
        window.render_frame(cx);
        assert!(
            !flags(&content, cx)[0].lock_transparency,
            "a base lock disables the partial toggles"
        );

        window.click("layer-lock-header-base", cx);
        window.render_frame(cx);
        assert!(!flags(&content, cx)[0].locked, "the base toggle unlocks");
        window.click("layer-lock-header-transparency", cx);
        window.render_frame(cx);
        assert!(
            flags(&content, cx)[0].lock_transparency,
            "the transparency toggle flips its own flag"
        );

        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        assert!(
            !flags(&content, cx)[0].lock_transparency,
            "undo restores the transparency flag"
        );

        window.render_frame(cx);
        assert!(
            window.try_find("layer-lock-header-base-tooltip").is_none(),
            "the lock tooltip is absent at rest"
        );
        window.hover("layer-lock-header-base", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("layer-lock-header-base-tooltip").is_none(),
            "the lock tooltip waits out the hover delay"
        );
    })
    .expect("test window remains open");
    settle_tooltip(cx, window, |window, _cx| {
        assert!(
            window.try_find("layer-lock-header-base-tooltip").is_some(),
            "hovering the base toggle shows its tooltip"
        );
    });
}
