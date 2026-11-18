//! The crop advanced group in the bar's overflow panel, and the crop request
//! it drives: the straighten angle, the rotation buttons, the composition
//! guides and what Apply sends to the core command.

use super::*;

/// The advanced group in the crop bar's overflow panel.
///
/// The crop bar mounts a More trigger and it opens the advanced group: the
/// straighten angle, the two quarter-turn buttons, the composition guides and
/// the Classic interaction switch.
///
/// The trigger carries `test_support()`, so the panel is found by id rather
/// than by a measured span.
#[gpui_kit::test]
fn the_crop_overflow_opens_the_advanced_group(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1536.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        for id in [
            "crop-ratio",
            "opt-crop-delete",
            "opt-crop-fill",
            "opt-more",
            "opt-cancel",
            "opt-apply",
        ] {
            assert!(
                window.try_find(id).is_some(),
                "{id} must mount on the crop bar"
            );
        }
        assert!(
            window.try_find("opt-crop-classic").is_none(),
            "the advanced group is closed until the trigger is pressed"
        );
        window.click("opt-more", cx);
        window.render_frame(cx);
        for id in [
            "crop-angle",
            "opt-crop-rotate-ccw",
            "opt-crop-rotate-cw",
            "crop-guide-none",
            "crop-guide-thirds",
            "crop-guide-grid",
            "crop-guide-diagonal",
            "crop-guide-golden",
            "opt-crop-classic",
        ] {
            assert!(
                window.try_find(id).is_some(),
                "{id} must be in the crop advanced group"
            );
        }
    })
    .expect("test window remains open");
}

/// The guide rows in the advanced group choose the composition guides.
#[gpui_kit::test]
fn the_guide_rows_choose_the_composition_guides(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        window.click("opt-more", cx);
        window.render_frame(cx);
        window.click("crop-guide-diagonal", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_guides,
            crate::crop_geometry::CropGuides::Diagonal
        );
    })
    .expect("test window remains open");
}

/// The quarter-turn buttons turn the picture inside the frame.
#[gpui_kit::test]
fn the_rotate_buttons_turn_the_picture(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        window.click("opt-more", cx);
        window.render_frame(cx);
        window.click("opt-crop-rotate-cw", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_rotation,
            -90.0,
            "the clockwise button turns the picture clockwise"
        );
        window.click("opt-crop-rotate-ccw", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_rotation,
            0.0,
            "the counter-clockwise button turns it back"
        );
    })
    .expect("test window remains open");
}

/// Apply sends the Size target to the core crop command.
#[gpui_kit::test]
fn apply_sends_the_size_target_to_core(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            app.state.update(cx, |state, _| {
                state.crop_mode = crate::crop_aspect::CropMode::Size;
                state.crop_size_target = Some((800, 600));
                state.crop_rect = Some([0.0, 0.0, 2.0, 1.0]);
            });
            app.apply_crop(cx).expect("crop applies");
        });
        let request = content.read(cx).state_for_test().read(cx).last_crop_request;
        let request = request.expect("the crop request is recorded");
        assert_eq!(
            request.target,
            Some((800.0, 600.0)),
            "the Size mode writes the output size"
        );
    })
    .expect("test window remains open");
}

/// Apply sends the straighten angle to the core crop command.
#[gpui_kit::test]
fn apply_sends_the_rotation_to_core(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            app.state.update(cx, |state, _| {
                state.crop_rotation = 12.5;
                state.crop_rect = Some([0.0, 0.0, 2.0, 1.0]);
            });
            app.apply_crop(cx).expect("crop applies");
        });
        let request = content.read(cx).state_for_test().read(cx).last_crop_request;
        let request = request.expect("the crop request is recorded");
        assert_eq!(
            request.rotation,
            Some(-12.5),
            "Modern apply sends the negated straighten angle to core"
        );
    })
    .expect("test window remains open");
}

/// The crop may reach past the page edge; the document grows to the crop.
#[gpui_kit::test]
fn apply_crops_past_the_page_edge(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            app.state.update(cx, |state, _| {
                state.crop_rect = Some([-1.0, 0.0, 4.0, 1.0]);
            });
            app.apply_crop(cx).expect("crop applies");
        });
        let request = content.read(cx).state_for_test().read(cx).last_crop_request;
        let request = request.expect("the crop request is recorded");
        assert_eq!(request.x, -1.0, "the crop starts before the page");
        assert_eq!(request.width, 4.0, "the crop is wider than the page");
    })
    .expect("test window remains open");
}
