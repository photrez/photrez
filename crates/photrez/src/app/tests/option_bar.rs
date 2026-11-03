//! The option bar under live tool state: which controls mount inline, which
//! fold into the overflow dropdown, and what a stepper or tooltip does to the
//! running tool.

use super::input::set_tool;
use super::*;
use crate::state::ToolId;
use gpui_kit::test::TestWindowExt as _;

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
        assert!(
            window.try_find("opt-auto-select-tooltip").is_some(),
            "the auto-select toggle carries its tooltip"
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
