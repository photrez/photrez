//! The option bar under live tool state: which controls mount inline, which
//! fold into the overflow dropdown, and what a stepper or tooltip does to the
//! running tool.

use super::input::{drag_doc, history_cursor, pixels, set_tool, tab_to_canvas};
use super::*;
use crate::state::ToolId;
use gpui_kit::test::TestWindowExt as _;

/// One shared text-layer creation leg: the fixture bar must drive a real
/// non-background layer, so the tests type one character into a text drag.
fn create_typed_text_layer(
    window: &mut gpui_kit::Window,
    content: &Entity<App>,
    cx: &mut gpui_kit::App,
) {
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
}

#[gpui_kit::test]
fn option_bar_steppers_change_the_live_tool_settings(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 1.0);
        window.render_frame(cx);
        window.click("opt-brush-opacity", cx);
        window.press("9", cx);
        window.press("0", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        let opacity = content.read(cx).state_for_test().read(cx).brush_opacity;
        assert!((opacity - 0.9).abs() < 1e-6, "opacity={opacity}");
        window.click("opt-more", cx);
        window.render_frame(cx);
        window.click("opt-brush-smoothing", cx);
        window.press("5", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).brush_smoothing,
            5.0
        );

        set_tool(&content, cx, ToolId::PaintBucket, 1.0);
        window.render_frame(cx);
        window.click("opt-tol-64", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).fill_tolerance,
            64
        );
    })
    .expect("test window remains open");
}
/// Below the 880 px container breakpoint the brush bar folds Flow,
/// Smoothing, Presets and the eraser's full-strength helper into `MoreDropdown`;
/// at 1366x820 the bar resolves to 781 px, so this is the live layout.
#[gpui_kit::test]
fn brush_overflow_folds_into_the_more_dropdown_below_880(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 1.0);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-more").is_some(),
            "the overflow button mounts"
        );
        for id in [
            "opt-brush-flow",
            "opt-brush-smoothing",
            "opt-brush-preset",
            "opt-brush-hard100",
        ] {
            assert!(
                window.try_find(id).is_none(),
                "{id} must stay in the overflow at 781 px"
            );
        }

        window.click("opt-more", cx);
        window.render_frame(cx);
        for id in [
            "opt-more-panel",
            "opt-brush-flow",
            "opt-brush-smoothing",
            "opt-brush-preset",
        ] {
            assert!(
                window.try_find(id).is_some(),
                "the overflow panel carries {id}"
            );
        }
        assert!(
            window.try_find("opt-more-hard100").is_none(),
            "the full-strength helper is eraser only"
        );
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-more-panel").is_none(),
            "Escape closes the overflow panel"
        );

        set_tool(&content, cx, ToolId::Eraser, 1.0);
        window.render_frame(cx);
        window.click("opt-more", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-more-hard100").is_some(),
            "the eraser overflow carries Hard 100"
        );
        super::tools::set_state(&content, cx, |state| {
            state.eraser_hardness = 0.2;
            state.eraser_opacity = 0.3;
            state.eraser_flow = 0.4;
            state.brush_preset_index = 2;
        });
        window.render_frame(cx);
        window.click("opt-more-hard100", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(state.eraser_hardness, 1.0, "Hard 100 sets full hardness");
        assert_eq!(state.eraser_opacity, 1.0, "Hard 100 sets full strength");
        assert_eq!(state.eraser_flow, 1.0, "Hard 100 sets full flow");
        assert_eq!(
            state.brush_preset_index,
            usize::MAX,
            "Hard 100 clears the preset to Custom"
        );
    })
    .expect("test window remains open");
}
#[gpui_kit::test]
fn option_bar_controls_show_hover_tooltips(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);
        window.hover("opt-shape-kind", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-shape-kind-tooltip").is_some(),
            "hovering the shape control shows its tooltip"
        );
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-shape-kind-tooltip").is_none(),
            "leaving the control hides the tooltip"
        );
        assert!(
            window.try_find("opt-fill-color").is_some(),
            "Fill defaults on, so the swatch is mounted"
        );
        window.hover("opt-fill-color", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-fill-color-tooltip").is_some(),
            "the fill swatch carries its fill-color tooltip"
        );
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);
        window.click("opt-shape-stroke", cx);
        window.render_frame(cx);
        window.click("opt-stroke-enable", cx);
        window.render_frame(cx);
        window.click("opt-fill", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-fill-color").is_none(),
            "the fill swatch unmounts once Fill is off"
        );
        window.click("opt-fill", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-fill-color").is_some(),
            "re-enabling Fill remounts the swatch"
        );
        window.hover("opt-shape-stroke", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-shape-stroke-tooltip").is_some(),
            "the stroke pill carries its stroke-options tooltip"
        );
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);

        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);
        window.hover("opt-weight", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-weight-tooltip").is_some(),
            "the weight control carries its own tooltip"
        );
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);
        window.hover("opt-italic", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-italic-tooltip").is_some(),
            "the italic button carries its tooltip"
        );
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);
        window.hover("opt-align-left", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-align-left-tooltip").is_some(),
            "the align-left button carries its tooltip"
        );
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);

        set_tool(&content, cx, ToolId::Brush, 1.0);
        window.render_frame(cx);
        window.click("opt-more", cx);
        window.render_frame(cx);
        window.hover("opt-brush-preset", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-brush-preset-tooltip").is_some(),
            "the brush preset control carries its tooltip"
        );
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-brush-preset-tooltip").is_none(),
            "leaving the preset control hides its tooltip"
        );

        set_tool(&content, cx, ToolId::Move, 1.0);
        window.render_frame(cx);
        window.hover("opt-auto-select", cx);
        window.render_frame(cx);
        let published = {
            let s = content.read(cx).state_for_test().read(cx);
            (s.tooltip.clone(), s.more_open, s.open_select, s.active_tool)
        };
        assert!(
            window.try_find("opt-auto-select-tooltip").is_some(),
            "the auto-select toggle carries its tooltip, published {published:?}"
        );
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-auto-select-tooltip").is_none(),
            "leaving the auto-select toggle hides its tooltip"
        );
        window.hover("opt-snap", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-snap-tooltip").is_some(),
            "the snap toggle carries its tooltip"
        );
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);
        window.hover("opt-transform", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-transform-tooltip").is_some(),
            "the transform toggle carries its tooltip"
        );
    })
    .expect("test window remains open");
}
/// The bar's container breakpoints are evaluated live, not just pinned: at a
/// content box of 880 px or more the brush Flow, Smoothing and preset controls
/// and the eraser's full-strength helper mount inline and the overflow trigger
/// is absent; below the breakpoint the same controls stay folded and the
/// trigger is mounted.
///
/// RED proof: force the wide gate off and the overflow gate on in the bar's
/// breakpoint helpers and the wide half of this test fails while the narrow
/// half still passes.
#[gpui_kit::test]
fn option_bar_at_or_above_880_mounts_the_folded_controls_inline(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    // 1600 px viewport: 52 px rail + 598 px dock leaves a 950 px bar, an
    // 926 px content box - above the 880 px breakpoint.
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1600.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 1.0);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-more").is_none(),
            "the overflow trigger is absent at or above the breakpoint"
        );
        for id in ["opt-brush-flow", "opt-brush-smoothing", "opt-brush-preset"] {
            assert!(
                window.try_find(id).is_some() && window.find(id).visible(),
                "{id} must be inline at or above the breakpoint"
            );
        }

        set_tool(&content, cx, ToolId::Eraser, 1.0);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-brush-hard100").is_some()
                && window.find("opt-brush-hard100").visible(),
            "the eraser's full-strength helper is inline at or above the breakpoint"
        );
    })
    .expect("wide test window remains open");

    // Mirror: at 1366 px the bar resolves to a 757 px content box, below the
    // breakpoint - the folded controls stay unmounted and the trigger mounts.
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1366.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 1.0);
        window.render_frame(cx);
        assert!(
            window.find("opt-more").visible(),
            "the overflow trigger is mounted below the breakpoint"
        );
        for id in [
            "opt-brush-flow",
            "opt-brush-smoothing",
            "opt-brush-preset",
            "opt-brush-hard100",
        ] {
            assert!(
                window.try_find(id).is_none(),
                "{id} must stay folded below the breakpoint"
            );
        }
    })
    .expect("narrow test window remains open");
}

/// The wide bar's full-strength helper is the eraser's click target above the
/// breakpoint; only the folded route had its effect proven.
#[gpui_kit::test]
fn inline_full_strength_helper_sets_the_eraser_to_full(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1600.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Eraser, 1.0);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-brush-hard100").is_some()
                && window.find("opt-brush-hard100").visible(),
            "the helper is inline at or above the breakpoint"
        );
        super::tools::set_state(&content, cx, |state| {
            state.eraser_hardness = 0.2;
            state.eraser_opacity = 0.3;
            state.eraser_flow = 0.4;
            state.brush_preset_index = 2;
        });
        window.render_frame(cx);
        window.click("opt-brush-hard100", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(
            state.eraser_hardness, 1.0,
            "the inline helper sets full hardness"
        );
        assert_eq!(
            state.eraser_opacity, 1.0,
            "the inline helper sets full strength"
        );
        assert_eq!(state.eraser_flow, 1.0, "the inline helper sets full flow");
        assert_eq!(
            state.brush_preset_index,
            usize::MAX,
            "the inline helper clears the preset to Custom"
        );
    })
    .expect("test window remains open");
}

/// The Move bar's narrow-width overflow is a real panel, not just the
/// trigger: clicking `opt-more` mounts the Align group (six directions), the
/// Flip group, and Reset Transform; Align Left rewrites the selected layer's
/// x to 0 through core history, Flip Horizontal toggles the layer's flip and
/// Reset clears the offsets. Escape and a click anywhere outside the panel
/// (the reference backdrop) dismiss it.
#[gpui_kit::test]
fn move_overflow_panel_aligns_and_resets_the_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        create_typed_text_layer(window, &content, cx);
        content.update(cx, |app, cx| {
            app.nudge_selected_layer(20.0, 5.0, cx)
                .expect("nudge applies");
        });
        set_tool(&content, cx, ToolId::Move, 1.0);
        // Align acts on the multi-selection set, which a canvas click or a
        // Layers row click writes; the text creation only sets the primary row.
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                let primary = state.selected_layer;
                state.selected_layers = vec![primary];
            });
        });
        window.render_frame(cx);
        assert!(
            window.try_find("opt-more").is_some(),
            "the narrow bar renders the overflow trigger"
        );
        assert!(
            window.try_find("opt-reset").is_none(),
            "the inline >=880 px group stays folded at 781 px"
        );

        window.click("opt-more", cx);
        window.render_frame(cx);
        for id in [
            "opt-more-panel",
            "opt-move-align-left",
            "opt-move-align-center-h",
            "opt-move-align-right",
            "opt-move-align-top",
            "opt-move-align-center-v",
            "opt-move-align-bottom",
            "opt-move-flip-h",
            "opt-move-flip-v",
            "opt-move-reset",
        ] {
            assert!(
                window.try_find(id).is_some(),
                "the overflow panel carries {id}"
            );
        }

        window.click("opt-move-reset", cx);
        window.render_frame(cx);
        let before = content
            .read(cx)
            .selected_transform_for_test(cx)
            .expect("the text layer carries a transform");
        let cursor_before_align = history_cursor(&content, cx);
        let panel_bounds = window
            .try_find("opt-more-panel")
            .map(|snapshot| snapshot.bounds());
        let button_bounds = window
            .try_find("opt-move-align-left")
            .map(|snapshot| snapshot.bounds());
        window.click("opt-move-align-left", cx);
        window.render_frame(cx);
        let after = content
            .read(cx)
            .selected_transform_for_test(cx)
            .expect("the text layer carries a transform");
        assert_eq!(
            after.0,
            0.0,
            "align left pins x to the canvas edge; diag {:?} panel {panel_bounds:?} button \
             {button_bounds:?} cursor {cursor_before_align} -> {}",
            {
                let s = content.read(cx).state_for_test().read(cx);
                (
                    s.selected_layer,
                    s.selected_layers.clone(),
                    s.layers
                        .iter()
                        .map(|row| (row.id, row.core_id.clone()))
                        .collect::<Vec<_>>(),
                    s.layer_flags.clone(),
                    s.status_error.clone(),
                    s.more_open,
                    s.active_tool,
                )
            },
            history_cursor(&content, cx)
        );
        assert_eq!(after.1, before.1, "align left leaves y unchanged");

        let flipped = |cx: &mut gpui_kit::App| {
            content
                .update(cx, |app, cx| {
                    app.selected_layer_for_cursor(cx)
                        .map(|(_, layer)| layer.transform.flip_h)
                })
                .expect("the panel leaves a selected layer")
        };
        let flip_cursor = history_cursor(&content, cx);
        assert!(!flipped(cx), "the layer starts unflipped");
        window.click("opt-move-flip-h", cx);
        window.render_frame(cx);
        assert!(flipped(cx), "the flip row toggles the layer transform");
        assert_eq!(
            history_cursor(&content, cx),
            flip_cursor + 1,
            "the flip commits one transform edit"
        );

        content.update(cx, |app, cx| {
            app.nudge_selected_layer(30.0, 12.0, cx)
                .expect("nudge applies");
        });
        window.render_frame(cx);
        window.click("opt-move-reset", cx);
        window.render_frame(cx);
        assert_eq!(
            content
                .read(cx)
                .selected_transform_for_test(cx)
                .map(|transform| (transform.0, transform.1)),
            Some((0.0, 0.0)),
            "reset clears the layer's offsets",
        );

        window.press("escape", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-more-panel").is_none(),
            "Escape closes the overflow panel"
        );

        window.click("opt-more", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-more-panel").is_some(),
            "the trigger reopens the panel"
        );
        window.click("canvas-hitbox", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-more-panel").is_none(),
            "a click outside the panel (the reference backdrop) closes it"
        );
    })
    .expect("test window remains open");
}

/// The Text bar's box-mode control is the reference's two-item list, not a
/// closed trigger: choosing Fixed Box rewrites the selected text layer's
/// `box_mode` to Area through core history (with a box width, as the
/// reference seeds), choosing Auto Width returns it to Point with the box
/// dimensions cleared; Escape and an outside click dismiss the list.
#[gpui_kit::test]
fn text_box_mode_list_switches_the_layer_between_modes(cx: &mut gpui_kit::TestAppContext) {
    use photrez_core::ptz::layer_params::TextBoxMode;
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        create_typed_text_layer(window, &content, cx);
        assert!(
            window.try_find("opt-box-mode").is_some(),
            "the text bar renders the box-mode trigger"
        );
        assert!(
            window.try_find("opt-box-mode-panel").is_none(),
            "the list starts closed"
        );

        window.click("opt-box-mode", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-box-mode-panel").is_some(),
            "the trigger opens the box-mode list"
        );
        assert!(window.try_find("opt-box-mode-point").is_some());
        assert!(window.try_find("opt-box-mode-area").is_some());

        window.click("opt-box-mode-area", cx);
        window.render_frame(cx);
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(
            data.box_mode,
            TextBoxMode::Area,
            "Fixed Box writes area mode to the layer"
        );
        assert!(
            data.box_width >= 1.0,
            "area mode carries a box width, got {}",
            data.box_width
        );
        assert!(
            window.try_find("opt-box-mode-panel").is_none(),
            "choosing an option closes the list"
        );

        window.click("opt-box-mode", cx);
        window.render_frame(cx);
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-box-mode-panel").is_none(),
            "Escape closes the box-mode list"
        );

        window.click("opt-box-mode", cx);
        window.render_frame(cx);
        window.click("canvas-hitbox", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-box-mode-panel").is_none(),
            "a click outside the list closes it"
        );

        window.click("opt-box-mode", cx);
        window.render_frame(cx);
        window.click("opt-box-mode-point", cx);
        window.render_frame(cx);
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(
            data.box_mode,
            TextBoxMode::Point,
            "Auto Width returns the layer to point mode"
        );
        assert_eq!(data.box_width, 0.0, "point mode clears the box width");
        assert_eq!(data.box_height, 0.0, "point mode clears the box height");
    })
    .expect("test window remains open");
}

/// The Selection bar's overflow trigger hosts the clipboard grid the reference
/// folds at narrow widths. With no
/// committed selection the rows stay inert (the reference's
/// `pointer-events-none`); once one exists, Invert rewrites it, Delete clears
/// the selected pixels through core history and Deselect drops it. Escape and
/// an outside click dismiss the panel. The reference's Transform toggle in
/// this panel is not covered: the native edit mode has no bar-click entry
/// point yet.
#[gpui_kit::test]
fn selection_overflow_panel_reaches_the_clipboard_actions(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    // The window size the captures use: the bar column is 886 px wide, so the
    // selection row folds (content box 861 px <= 880) and its overflow trigger
    // still sits inside the column clip.
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1536.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        let trigger = window
            .try_find("opt-more")
            .expect("the selection bar renders the overflow trigger");
        assert!(
            trigger.visible(),
            "the overflow trigger is clickable, got {trigger:?}"
        );
        window.click("opt-more", cx);
        window.render_frame(cx);
        for id in [
            "opt-more-panel",
            "opt-sel-cut",
            "opt-sel-copy",
            "opt-sel-paste",
            "opt-sel-invert",
            "opt-sel-delete",
            "opt-sel-deselect",
        ] {
            assert!(
                window.try_find(id).is_some(),
                "the overflow panel carries {id}"
            );
        }

        // No committed selection yet: the reference keeps the rows inert.
        window.click("opt-sel-copy", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).clipboard_for_test().is_none(),
            "Copy writes nothing without a committed selection"
        );

        content.update(cx, |app, cx| {
            app.select_all(cx).expect("select all commits");
        });
        window.render_frame(cx);
        assert!(
            content.read(cx).core_selection_for_test().is_some(),
            "the fixture carries a committed selection"
        );
        window.click("opt-sel-invert", cx);
        window.render_frame(cx);
        assert_eq!(
            content
                .read(cx)
                .core_selection_for_test()
                .and_then(|selection| selection.inverted),
            Some(true),
            "the panel Invert flips the full-document rect onto its complement, which selects \
             nothing"
        );

        content.update(cx, |app, cx| {
            app.select_all(cx).expect("select all commits");
        });
        window.render_frame(cx);
        let before = pixels(&content, cx).2;
        let cursor_before = history_cursor(&content, cx);
        window.click("opt-sel-delete", cx);
        window.render_frame(cx);
        let after = pixels(&content, cx).2;
        assert_ne!(after, before, "Delete clears the selected pixels");
        assert_eq!(
            history_cursor(&content, cx),
            cursor_before + 1,
            "Delete commits one history entry"
        );
        assert!(
            window.try_find("opt-more-panel").is_some(),
            "the panel stays open after acting, as in the reference"
        );

        content.update(cx, |app, cx| {
            app.select_all(cx).expect("select all commits");
        });
        window.render_frame(cx);
        window.click("opt-sel-deselect", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).core_selection_for_test().is_none(),
            "the panel Deselect clears the committed selection"
        );

        window.press("escape", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-more-panel").is_none(),
            "Escape closes the overflow panel"
        );

        window.click("opt-more", cx);
        window.render_frame(cx);
        window.click("canvas-hitbox", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-more-panel").is_none(),
            "a click outside the panel closes it"
        );
    })
    .expect("test window remains open");
}

/// What the Move bar's transform cells are showing: the app's own readout for
/// the layer on screen.
fn move_bar_readout(
    content: &Entity<App>,
    cx: &gpui_kit::App,
) -> crate::state::LayerTransformReadout {
    content
        .read(cx)
        .transform_readout(cx)
        .expect("a layer is on screen")
}

/// The Move bar's transform cells read and edit the layer the canvas is
/// holding: X and R submit, and Escape puts back the value the edit started
/// from, exactly as the reference's fields do.
#[gpui_kit::test]
fn the_move_bar_transform_fields_describe_and_edit_the_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Move, 1.0);
        window.render_frame(cx);
        let resting = move_bar_readout(&content, cx);
        assert_eq!(
            (resting.x, resting.y, resting.width, resting.height),
            (0.0, 0.0, 2.0, 1.0),
            "the readout is the fixture's own 2x1 layer, not a painted constant"
        );

        window.click("num-x", cx);
        window.press("4", cx);
        window.press("2", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        let moved = move_bar_readout(&content, cx);
        assert_eq!(moved.x, 42.0, "the X field submits the position");
        assert_eq!(
            (moved.width, moved.height),
            (2.0, 1.0),
            "an X edit leaves the frame size alone"
        );

        window.click("num-y", cx);
        window.render_frame(cx);
        window.press("9", cx);
        window.render_frame(cx);
        assert_eq!(
            move_bar_readout(&content, cx).y,
            9.0,
            "the field commits as the digits land"
        );
        window.press("escape", cx);
        window.render_frame(cx);
        assert_eq!(
            move_bar_readout(&content, cx).y,
            0.0,
            "Escape restores the value the edit started from"
        );

        window.click("num-r", cx);
        window.press("3", cx);
        window.press("0", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        let turned = move_bar_readout(&content, cx);
        assert_eq!(turned.rotation, 30.0, "the R field submits the rotation");
        assert_eq!(turned.x, 42.0, "a rotation edit leaves the position alone");
    })
    .expect("test window remains open");
}

/// A locked layer keeps the reference's disabled fields: the click opens no
/// edit and the value cannot move.
#[gpui_kit::test]
fn a_locked_layer_keeps_the_move_bar_transform_fields_inert(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Move, 1.0);
        content.update(cx, |app, cx| app.add_empty_layer(cx));
        let core_id = content
            .read(cx)
            .selected_layer_for_cursor(cx)
            .expect("the new layer is selected")
            .1
            .id;
        content.update(cx, |app, cx| app.toggle_layer_locked(core_id, cx));
        window.render_frame(cx);
        assert!(
            move_bar_readout(&content, cx).locked,
            "the readout carries the layer's own lock flag"
        );

        window.click("num-x", cx);
        window.press("4", cx);
        window.press("2", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            move_bar_readout(&content, cx).x,
            0.0,
            "a locked layer answers no transform edit"
        );
    })
    .expect("test window remains open");
}
