//! Brush presets and preview patches, and the Move bar's toggles, snap state and
//! auto-select.

use super::*;

#[gpui_kit::test]
fn brush_preset_button_cycles_the_pinned_presets(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 1.0);
        window.render_frame(cx);
        // The fixture bar is 781 px: the preset control lives in the overflow.
        window.click("opt-more", cx);
        window.render_frame(cx);
        window.click("opt-brush-preset", cx);
        window.render_frame(cx);
        window.click("opt-brush-preset-option-1", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.brush_size, 40.0);
            assert!((state.brush_hardness - 0.15).abs() < 1e-6);
        }
        window.click("opt-brush-preset", cx);
        window.render_frame(cx);
        window.click("opt-brush-preset-option-2", cx);
        window.render_frame(cx);
        assert_eq!(content.read(cx).state_for_test().read(cx).brush_size, 5.0);

        set_tool(&content, cx, ToolId::Eraser, 1.0);
        window.render_frame(cx);
        window.click("opt-brush-preset", cx);
        window.render_frame(cx);
        window.click("opt-brush-preset-option-4", cx);
        window.render_frame(cx);
        assert_eq!(content.read(cx).state_for_test().read(cx).eraser_size, 30.0);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn brush_pointer_moves_push_preview_patches_without_recompositing(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 20.0);
        window.render_frame(cx);
        let before = content.read(cx).full_composites_for_test();
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.2, 0.5), (0.8, 0.5), cx);
        let after = content.read(cx).full_composites_for_test();
        assert_eq!(
            after, before,
            "a brush gesture never recomposites the document; moves and commit both patch"
        );
        content.update(cx, |app, cx| {
            app.refresh_active_canvas(cx).expect("composite runs");
        });
        assert_eq!(
            content.read(cx).full_composites_for_test(),
            after + 1,
            "the counter observes a real full composite"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn multi_layer_brush_moves_push_preview_patches_without_recompositing(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_core_layer_count_for_test(),
            Some(2),
            "the fixture has two raster layers"
        );
        let composites_before = content.read(cx).full_composites_for_test();
        let previews_before = content.read(cx).preview_pushes_for_test();
        drag_doc(window, (2, 1), (0.5, 0.5), (1.5, 0.5), cx);
        assert!(
            content.read(cx).preview_pushes_for_test() > previews_before,
            "a move on a multi-layer document pushes a display preview"
        );
        assert_eq!(
            content.read(cx).full_composites_for_test(),
            composites_before + 1,
            "only the pointer-up commit recomposites; the moves patch"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn move_toggles_are_tab_reachable_and_space_operates_them(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Move, 1.0);
        window.render_frame(cx);
        tab_to_canvas(window, cx);

        let ids = ["opt-snap", "opt-auto-select", "opt-transform"];
        let mut seen = [false; 3];
        for _ in 0..12 {
            window.press("tab", cx);
            window.render_frame(cx);
            for (index, id) in ids.iter().enumerate() {
                if window.find(*id).focused() == Some(true) {
                    seen[index] = true;
                }
            }
            if seen.iter().all(|reached| *reached) {
                break;
            }
        }
        assert!(
            seen.iter().all(|reached| *reached),
            "Tab must reach every Move toggle: {seen:?}"
        );

        // One cycle of the tab order now carries every inline numeric row too, so
        // coming back round to the snap toggle takes longer than it used to.
        for _ in 0..64 {
            if window.find("opt-snap").focused() == Some(true) {
                break;
            }
            window.press("tab", cx);
            window.render_frame(cx);
        }
        assert_eq!(
            window.find("opt-snap").focused(),
            Some(true),
            "the snap toggle is focused before Space"
        );
        let before = content.read(cx).state_for_test().read(cx).snap_enabled;
        window.press("space", cx);
        let after_press = content.read(cx).state_for_test().read(cx).snap_enabled;
        assert_eq!(after_press, !before, "Space toggles before the next frame");
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).snap_enabled,
            !before,
            "Space operates the focused toggle"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn move_snap_toggle_updates_snap_state(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Move, 1.0);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).snap_enabled,
            "snapping defaults to on"
        );
        window.click("opt-snap", cx);
        window.render_frame(cx);
        assert!(
            !content.read(cx).state_for_test().read(cx).snap_enabled,
            "clicking the snap toggle turns snapping off"
        );
        window.click("opt-snap", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).snap_enabled,
            "clicking again turns snapping back on"
        );
        window.click("opt-auto-select", cx);
        window.render_frame(cx);
        assert!(
            !content.read(cx).state_for_test().read(cx).move_auto_select,
            "the auto-select toggle turns auto-select off"
        );
        window.click("opt-auto-select", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).move_auto_select,
            "clicking again turns auto-select back on"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn transform_controls_toggle_gates_direct_manipulation(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test()
                .update(cx, |state, _| state.active_tool = ToolId::Move);
        });
        window.render_frame(cx);
        let center = (200.0_f32, 150.0_f32);
        let moved = (center.0 + 40.0, center.1 + 30.0);

        window.click("opt-transform", cx);
        window.render_frame(cx);
        assert!(
            !content
                .read(cx)
                .state_for_test()
                .read(cx)
                .show_transform_controls
        );
        let before = content.read(cx).selected_transform_for_test(cx);
        drag_doc(window, (400, 300), center, moved, cx);
        let after = content.read(cx).selected_transform_for_test(cx);
        assert_eq!(
            after, before,
            "transform controls off blocks direct manipulation"
        );

        window.click("opt-transform", cx);
        window.render_frame(cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .show_transform_controls
        );
        // Direct manipulation acts on the selected layer. The drag above was a
        // layer marquee over a canvas whose only layer is the Background,
        // which the marquee deliberately skips, so it cleared the projected
        // selection. Re-select through the layers panel row - the user-visible
        // path - so the two phases vary only the transform-controls toggle:
        // with no layer selected, an on-toggle drag transforms nothing either.
        let (rows, _) = layer_rows(&content, cx);
        let background = rows
            .iter()
            .find(|row| row.core_id.is_some())
            .expect("the blank document projects a Background row");
        window.click(format!("layer-row-{}", background.id), cx);
        window.render_frame(cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .selected_layers
                .contains(&background.id),
            "the row click selects the layer the gate-on drag will manipulate"
        );
        drag_doc(window, (400, 300), center, moved, cx);
        let after_on = content.read(cx).selected_transform_for_test(cx);
        assert_ne!(
            after_on, before,
            "transform controls on restores drag-to-move on the selected layer"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn move_auto_select_click_picks_the_layer_under_the_pointer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Move, 1.0);
        window.render_frame(cx);
        let (rows, selected) = layer_rows(&content, cx);
        assert_eq!(rows.len(), 2, "the fixture has two layers");
        assert_eq!(selected, rows[0].id, "the top layer starts selected");

        // The top layer is 1x1 at the document origin, so the right half only
        // intersects the bottom layer.
        drag_doc(window, (2, 1), (1.5, 0.5), (1.5, 0.5), cx);
        let (_, selected) = layer_rows(&content, cx);
        assert_eq!(
            selected, rows[1].id,
            "the click selects the layer under the pointer"
        );

        drag_doc(window, (2, 1), (0.5, 0.5), (0.5, 0.5), cx);
        let (_, selected) = layer_rows(&content, cx);
        assert_eq!(
            selected, rows[0].id,
            "the click selects the top layer again"
        );

        window.click("opt-auto-select", cx);
        window.render_frame(cx);
        assert!(!content.read(cx).state_for_test().read(cx).move_auto_select);
        drag_doc(window, (2, 1), (1.5, 0.5), (1.5, 0.5), cx);
        let (_, selected) = layer_rows(&content, cx);
        assert_eq!(
            selected, rows[0].id,
            "auto-select off keeps the current selection"
        );
    })
    .expect("test window remains open");
}
