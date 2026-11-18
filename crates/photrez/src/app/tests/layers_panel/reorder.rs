//! The row chevrons reorder through core and stop at the ends, with the move-layer tooltip on hover.

use super::*;

#[gpui_kit::test]
fn row_chevrons_reorder_through_core_and_stop_at_the_ends(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let chevron_bottom = std::cell::Cell::new(0u64);
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    // Row ids are projection positions and are reassigned on every
    // reprojection, so order is asserted through `core_id`, which is stable.
    let core_ids = |content: &Entity<App>, cx: &mut gpui_kit::App| -> Vec<String> {
        super::canvas_input::layer_rows(content, cx)
            .0
            .into_iter()
            .map(|row| row.core_id.expect("the fixture layers have core ids"))
            .collect()
    };
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (rows, _) = super::canvas_input::layer_rows(&content, cx);
        assert_eq!(rows.len(), 2, "the fixture opens two layers");
        assert_eq!(
            rows[0].core_id.as_deref(),
            Some("top"),
            "the projection lists the top layer first"
        );
        let top = rows[0].id;
        let bottom = rows[1].id;
        chevron_bottom.set(bottom);

        window.click(format("layer-up-{bottom}"), cx);
        window.render_frame(cx);
        assert_eq!(
            core_ids(&content, cx),
            ["bottom", "top"],
            "the second row moves above the first"
        );

        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        window.render_frame(cx);
        assert_eq!(
            core_ids(&content, cx),
            ["top", "bottom"],
            "undo restores the core order"
        );

        window.click(format("layer-up-{top}"), cx);
        window.click(format("layer-down-{bottom}"), cx);
        window.render_frame(cx);
        assert_eq!(
            core_ids(&content, cx),
            ["top", "bottom"],
            "the end chevrons cannot reorder"
        );

        assert!(
            window
                .try_find(format("layer-up-{bottom}-tooltip"))
                .is_none(),
            "the chevron is tooltip-free until hovered"
        );
        window.hover(format("layer-up-{bottom}"), cx);
        window.render_frame(cx);
        assert!(
            window
                .try_find(format("layer-up-{bottom}-tooltip"))
                .is_none(),
            "the chevron tooltip waits out the hover delay"
        );
    })
    .expect("test window remains open");
    settle_tooltip(cx, window, |window, _cx| {
        let bottom = chevron_bottom.get();
        assert!(
            window
                .try_find(format("layer-up-{bottom}-tooltip"))
                .is_some(),
            "hovering the chevron shows the move-layer tooltip"
        );
    });
}
