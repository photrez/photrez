//! The crop ratio and Size modes: the dropdown that refits the frame, the
//! lock of the current shape and the 300 PPI unit conversion of the Size
//! fields.

use super::*;

/// The ratio dropdown is live. Choosing 1:1 (inside the list's 300 px
/// viewport) refits the pending frame to the largest square inside the
/// document and records the recent.
#[gpui_kit::test]
fn crop_ratio_dropdown_refits_the_pending_frame(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        window.click("crop-ratio", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).crop_ratio_open,
            "the ratio list expands"
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_mode,
            crate::crop_aspect::CropMode::Free
        );
        let _active_row = window.find("crop-ratio-free");
        window.click("crop-ratio-preset-0", cx);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(
            state.crop_mode,
            crate::crop_aspect::CropMode::Ratio,
            "picking a preset switches to Ratio"
        );
        assert_eq!(state.crop_aspect, Some((1, 1)));
        assert_eq!(state.recent_crop_ratios, vec![(1, 1)]);
        let rect = state.crop_rect.expect("the preset refits the frame");
        assert!(
            (rect[2] - rect[3]).abs() < 1.0,
            "the preset keeps a square frame: {rect:?}"
        );
        assert!(rect[2] >= 1.0 && rect[3] >= 1.0, "the frame is non-empty");
    })
    .expect("test window remains open");
}

/// `Lock Current Shape` is inert outside Free: the control is disabled unless
/// the mode is Free; the Size mode keeps its mode.
#[gpui_kit::test]
fn crop_lock_shape_is_inert_outside_free_mode(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        window.click("crop-ratio", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        window.click("crop-ratio-size", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_mode,
            crate::crop_aspect::CropMode::Size,
            "the Size row switches the mode"
        );
        window.click("crop-ratio", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        window.click("crop-ratio-lock", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(
            state.crop_mode,
            crate::crop_aspect::CropMode::Size,
            "a disabled Lock Current Shape row must not switch the mode"
        );
        assert!(
            state.crop_ratio_open,
            "an inert row must not dismiss the list either"
        );
    })
    .expect("test window remains open");
}

/// The Size mode converts entry at the crop bar's 300 PPI base,
/// keeps the pixel target authoritative across unit changes and swap.
#[gpui_kit::test]
fn crop_size_mode_converts_at_300_ppi_and_swaps(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        window.click("crop-ratio", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).crop_ratio_open,
            "the ratio list opens before choosing Size"
        );
        window.click("crop-ratio-size", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(
                state.crop_mode,
                crate::crop_aspect::CropMode::Size,
                "the Size row switches the mode; open={}",
                state.crop_ratio_open
            );
            assert_eq!(state.crop_size_target, Some((800, 600)));
            assert_eq!(
                (state.crop_size_w.as_str(), state.crop_size_h.as_str()),
                ("800", "600")
            );
        }

        // Switch the unit to inches: the pixel target stays 800x600.
        window.click("crop-size-unit", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        window.click("crop-size-unit-in", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.crop_size_unit, crate::crop_aspect::CropSizeUnit::In);
            assert_eq!(state.crop_size_target, Some((800, 600)));
            assert_eq!(
                (state.crop_size_w.as_str(), state.crop_size_h.as_str()),
                ("2.67", "2.00"),
                "800 px is 2.67 in at 300 PPI"
            );
        }

        // Type 4 into W: 4 in * 300 = 1200 px.
        window.click("crop-size-w", cx);
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        // The field selects its value on focus, so the first digit replaces it.
        window.press("4", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.crop_size_target, Some((1200, 600)), "4 in at 300 PPI");
            assert_eq!(state.crop_size_w, "4");
        }

        // Swap: 1200x600 becomes 600x1200, and the buffers swap with it.
        window.click("crop-size-swap", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(state.crop_size_target, Some((600, 1200)));
        assert_eq!(
            (state.crop_size_w.as_str(), state.crop_size_h.as_str()),
            ("2.00", "4")
        );
    })
    .expect("test window remains open");
}
