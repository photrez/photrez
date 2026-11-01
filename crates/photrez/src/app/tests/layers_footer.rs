//! The layers panel's footer action buttons: what each icon button does
//! to the document, and the enabled/disabled gate around it.

use super::input::{mount_archive_app, select_layer};
use super::*;
use gpui_kit::test::TestWindowExt as _;

#[gpui_kit::test]
fn footer_adds_and_deletes_layers_with_undo(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(super::layers_panel::layers_len(&content, cx), 2);
        window.click("layer-action-0", cx);
        window.render_frame(cx);
        assert_eq!(
            super::layers_panel::layers_len(&content, cx),
            3,
            "add creates an empty layer"
        );
        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        assert_eq!(
            super::layers_panel::layers_len(&content, cx),
            2,
            "undo removes the added layer"
        );

        let (rows, selected) = super::input::layer_rows(&content, cx);
        let selected_core = rows
            .iter()
            .find(|row| row.id == selected)
            .and_then(|row| row.core_id.clone())
            .expect("a core layer is selected");
        window.click("layer-action-4", cx);
        window.render_frame(cx);
        assert_eq!(
            super::layers_panel::layers_len(&content, cx),
            1,
            "delete removes the layer"
        );
        assert!(
            !content
                .read(cx)
                .active_core_layer_ids_for_test()
                .contains(&selected_core),
            "the selected core layer is gone"
        );
        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        assert_eq!(
            super::layers_panel::layers_len(&content, cx),
            2,
            "undo restores the layer"
        );
    })
    .expect("test window remains open");
}
#[gpui_kit::test]
fn footer_actions_disable_on_the_background_and_last_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with_background(),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (rows, _) = super::input::layer_rows(&content, cx);
        let bottom = rows
            .iter()
            .find(|row| row.core_id.as_deref() == Some("bottom"))
            .expect("bottom row")
            .id;
        select_layer(&content, cx, bottom);
        window.render_frame(cx);

        window.click("layer-action-4", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert!(
                state.status_error.is_none(),
                "the Background delete is disabled, not a rejected command"
            );
            assert_eq!(state.layers.len(), 2, "the disabled delete removes nothing");
        }

        window.click("layer-action-2", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert!(
                state.status_error.is_none(),
                "merge-down is disabled with no layer below"
            );
            assert_eq!(state.layers.len(), 2, "the disabled merge changes nothing");
        }

        assert!(
            window.try_find("layer-action-0-tooltip").is_none(),
            "the footer tooltip is absent at rest"
        );
        window.hover("layer-action-0", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("layer-action-0-tooltip").is_some(),
            "hovering a footer action shows its tooltip"
        );
    })
    .expect("test window remains open");
}
/// The footer's duplicate icon button proves both sides of the disabled gate:
/// with an active layer the click runs the handler and duplicates it; with no
/// active layer the button is disabled and the same click must not run it,
/// even though the document still holds a raster layer the command would use.
///
/// RED proof: install the handler regardless of the disabled flag in the
/// actions footer and the disabled half fails - the inert click duplicates.
#[gpui_kit::test]
fn footer_duplicate_button_runs_only_while_enabled(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (rows, selected) = super::input::layer_rows(&content, cx);
        assert_eq!(rows.len(), 2, "the archive projects two rows");

        select_layer(&content, cx, selected);
        window.render_frame(cx);
        window.click("layer-action-1", cx);
        window.render_frame(cx);
        assert_eq!(
            super::layers_panel::layers_len(&content, cx),
            3,
            "the enabled duplicate button runs its handler"
        );

        // No row is active, so the button is disabled. The document still has
        // raster layers, so a leaked handler on this click would add a layer.
        select_layer(&content, cx, u64::MAX);
        window.render_frame(cx);
        window.click("layer-action-1", cx);
        window.render_frame(cx);
        assert_eq!(
            super::layers_panel::layers_len(&content, cx),
            3,
            "the disabled duplicate button must not run its handler"
        );
    })
    .expect("test window remains open");
}
