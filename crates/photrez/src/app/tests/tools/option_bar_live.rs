//! Option-bar controls that have to edit live state: tolerance, shape, gradient,
//! bucket, eyedropper and the text fields.

use super::*;

#[gpui_kit::test]
fn tolerance_slider_sets_the_value_on_click_and_drag(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::PaintBucket, 1.0);
        window.render_frame(cx);
        let bounds = window.find("opt-tolerance-slider").bounds();
        let width = f32::from(bounds.size.width);
        let height = f32::from(bounds.size.height);
        assert!(width > 0.0 && height > 0.0, "the track is laid out");

        window.click_at(
            "opt-tolerance-slider",
            gpui_kit::point(gpui_kit::px(width * 0.75), gpui_kit::px(height * 0.5)),
            cx,
        );
        window.render_frame(cx);
        let value = content.read(cx).state_for_test().read(cx).fill_tolerance;
        assert!(
            (i64::from(value) - 191).abs() <= 2,
            "click at 75% maps to 191, got {value}"
        );

        window.drag(
            bounds.origin + gpui_kit::point(gpui_kit::px(width * 0.25), gpui_kit::px(height * 0.5)),
            bounds.origin + gpui_kit::point(gpui_kit::px(width * 0.5), gpui_kit::px(height * 0.5)),
            cx,
        );
        window.render_frame(cx);
        let value = content.read(cx).state_for_test().read(cx).fill_tolerance;
        assert!(
            (i64::from(value) - 128).abs() <= 3,
            "drag to 50% maps to 128, got {value}"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn shape_bar_kind_select_is_live(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);
        window.click("opt-shape-kind", cx);
        window.render_frame(cx);
        window.click("opt-shape-kind-option-1", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).shape_kind,
            ShapeTool::Ellipse,
            "choosing Ellipse sets the shape kind"
        );
        assert_eq!(content.read(cx).state_for_test().read(cx).open_select, None);

        window.click("opt-shape-kind", cx);
        window.render_frame(cx);
        window.press("escape", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).open_select,
            None,
            "Escape closes the shape dropdown"
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).shape_kind,
            ShapeTool::Ellipse
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn shape_fill_stroke_radius_and_arrow_are_live(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);

        // The pill opens the popover; its header checkbox toggles the outline,
        // and the last-paint guard keeps fill or stroke on.
        window.click("opt-fill", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).shape_fill,
            "fill stays on while stroke is off"
        );
        window.click("opt-shape-stroke", cx);
        window.render_frame(cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .stroke_popover_open,
            "the pill opens the stroke popover"
        );
        window.click("opt-stroke-enable", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).shape_stroke,
            "the popover checkbox turns the outline on"
        );
        window.click("opt-fill", cx);
        window.render_frame(cx);
        assert!(
            !content.read(cx).state_for_test().read(cx).shape_fill,
            "fill can turn off once stroke is on"
        );
        window.click("opt-stroke-enable", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).shape_stroke,
            "stroke stays on while fill is off"
        );

        window.click("opt-shape-radius", cx);
        window.press("4", cx);
        window.press("2", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).shape_radius,
            42.0,
            "radius commits live from the field"
        );
        window.click("opt-shape-radius", cx);
        window.press("9", cx);
        window.press("9", cx);
        window.press("9", cx);
        window.press("enter", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).shape_radius,
            100.0,
            "radius clamps to the 100 ceiling"
        );

        window.click("opt-shape-kind", cx);
        window.render_frame(cx);
        window.click("opt-shape-kind-option-2", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).shape_kind,
            ShapeTool::Line
        );
        window.click("opt-shape-arrow", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).shape_arrow_head,
            "the line shape reveals the live arrow toggle"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn shape_bar_edits_the_selected_shape_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.1, 0.1), (0.9, 0.9), cx);

        content.update(cx, |app, cx| {
            let row = {
                let state = app.state_for_test().read(cx);
                state
                    .layers
                    .iter()
                    .find(|row| row.name == "Shape")
                    .map(|row| row.id)
                    .expect("shape row")
            };
            app.state_for_test().update(cx, |state, _| {
                state.selected_layer = row;
                state.selected_layers = vec![row];
            });
            app.project_layers(cx);
        });
        window.render_frame(cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .selected_shape
                .is_some(),
            "a selected shape puts the bar into edit mode"
        );
        let cursor_start = content.read(cx).active_core_history_cursor_for_test();

        // Enabling the outline through the popover edits the layer, not just
        // the next-shape defaults.
        window.click("opt-shape-stroke", cx);
        window.render_frame(cx);
        window.click("opt-stroke-enable", cx);
        window.render_frame(cx);
        let params = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .selected_shape
            .clone()
            .expect("shape parameters");
        assert!(
            params.stroke.enabled,
            "the popover checkbox reaches the layer"
        );
        assert_eq!(
            content.read(cx).active_core_history_cursor_for_test(),
            cursor_start.map(|cursor| cursor + 1),
            "the layer edit is one core history entry"
        );

        window.click("opt-stroke-color", cx);
        window.render_frame(cx);
        for digit in ["1", "2", "3", "4", "5", "6"] {
            window.press(digit, cx);
        }
        window.press("enter", cx);
        window.render_frame(cx);
        let params = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .selected_shape
            .clone()
            .expect("shape parameters");
        assert_eq!(
            params.stroke.color, "#123456",
            "the picker writes the layer's outline colour"
        );
        assert!(params.stroke.enabled);

        // Fill off is legal while the outline is on, and undo restores it.
        window.click("opt-fill", cx);
        window.render_frame(cx);
        let params = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .selected_shape
            .clone()
            .expect("shape parameters");
        assert_eq!(
            params.fill.kind,
            ShapeFillKind::None,
            "turning Fill off edits the layer"
        );
        content.update(cx, |app, cx| {
            assert!(app.step_history(true, cx).expect("undo fill"));
            assert_eq!(
                app.state_for_test()
                    .read(cx)
                    .selected_shape
                    .as_ref()
                    .map(|params| params.fill.kind.clone()),
                Some(ShapeFillKind::Solid),
                "undo restores the layer's fill"
            );
        });

        // The radius field commits to the layer and keeps the next-shape
        // default in step.
        window.click("opt-shape-stroke", cx);
        window.render_frame(cx);
        window.click("opt-shape-radius", cx);
        window.press("4", cx);
        window.press("2", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(
                state.selected_shape.as_ref().map(|params| params.radius),
                Some(42.0),
                "the radius field edits the selected layer"
            );
            assert_eq!(
                state.shape_radius, 42.0,
                "the edit syncs the session default"
            );
        }
        content.update(cx, |app, cx| {
            // The NumField commits live per keystroke, so "4" and "42" are
            // separate core entries.
            assert!(app.step_history(true, cx).expect("undo the 42"));
            assert_eq!(
                app.state_for_test()
                    .read(cx)
                    .selected_shape
                    .as_ref()
                    .map(|params| params.radius),
                Some(4.0),
                "undo steps back through the live radius commit"
            );
            assert!(app.step_history(true, cx).expect("undo the 4"));
            assert_eq!(
                app.state_for_test()
                    .read(cx)
                    .selected_shape
                    .as_ref()
                    .map(|params| params.radius),
                Some(0.0),
                "undo restores the layer's created radius"
            );
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn shape_stroke_popover_edits_color_and_width(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-stroke-popover").is_none(),
            "the popover starts closed"
        );
        window.click("opt-shape-stroke", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-stroke-popover").is_some(),
            "the pill mounts the popover"
        );

        window.click("opt-stroke-enable", cx);
        window.render_frame(cx);
        assert!(content.read(cx).state_for_test().read(cx).shape_stroke);

        window.click("opt-stroke-width", cx);
        window.press("1", cx);
        window.press("2", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .shape_stroke_width,
            12.0,
            "width commits live from the popover field"
        );
        window.click("opt-stroke-width", cx);
        window.press("9", cx);
        window.press("9", cx);
        window.press("9", cx);
        window.press("enter", cx);
        assert_eq!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .shape_stroke_width,
            200.0,
            "width clamps to the 200 ceiling"
        );

        window.click("opt-stroke-color", cx);
        window.render_frame(cx);
        assert!(
            window.find("color-picker").visible(),
            "the colour button opens the picker"
        );
        content.update(cx, |app, cx| {
            assert_eq!(
                app.state_for_test()
                    .read(cx)
                    .color_picker
                    .as_ref()
                    .map(|session| session.target),
                Some(crate::state::ColorTarget::ShapeStroke),
                "the picker edits the stroke target"
            );
        });
        for digit in ["1", "2", "3", "4", "5", "6"] {
            window.press(digit, cx);
        }
        window.press("enter", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(
                state.shape_stroke_color,
                [0x12, 0x34, 0x56, 255],
                "the picked colour reaches the stroke setting"
            );
            assert!(state.shape_stroke, "choosing a colour enables the outline");
        }

        // Close the popover, then draw: the new layer carries the popover's
        // width and colour through `apply_shape`.
        window.click("opt-shape-stroke", cx);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.1, 0.1), (0.9, 0.9), cx);
        assert_eq!(
            content.read(cx).last_shape_stroke_for_test(),
            Some((true, "#123456".to_string(), 200.0)),
            "the created layer uses the popover's stroke"
        );

        window.click("opt-shape-stroke", cx);
        window.render_frame(cx);
        window.press("escape", cx);
        assert!(
            !content
                .read(cx)
                .state_for_test()
                .read(cx)
                .stroke_popover_open,
            "Escape closes the popover"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn gradient_bar_selects_and_reverse_are_live(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Gradient, 1.0);
        window.render_frame(cx);
        assert!(
            window.try_find("option-live-gradient-radial").is_none(),
            "the Gradient option bar offers no live strip"
        );

        window.click("opt-gradient-type", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).open_select,
            Some("opt-gradient-type")
        );
        window.click("opt-gradient-type-option-1", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).gradient_radial,
            "choosing Radial sets the gradient type"
        );
        assert_eq!(content.read(cx).state_for_test().read(cx).open_select, None);

        window.click("opt-gradient-preset", cx);
        window.render_frame(cx);
        window.click("opt-gradient-preset-option-1", cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .gradient_fg_transparent,
            "choosing FG to transparent sets the preset"
        );

        assert!(!content.read(cx).state_for_test().read(cx).gradient_reverse);
        window.click("opt-reverse", cx);
        assert!(
            content.read(cx).state_for_test().read(cx).gradient_reverse,
            "the chrome Reverse button drives the setting"
        );

        window.click("opt-gradient-type", cx);
        window.render_frame(cx);
        window.press("escape", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).open_select,
            None,
            "Escape closes the dropdown without choosing"
        );
        assert!(content.read(cx).state_for_test().read(cx).gradient_radial);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn tolerance_slider_arrow_keys_adjust_the_value(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::PaintBucket, 1.0);
        window.render_frame(cx);
        let bounds = window.find("opt-tolerance-slider").bounds();
        let width = f32::from(bounds.size.width);
        let height = f32::from(bounds.size.height);
        window.click_at(
            "opt-tolerance-slider",
            gpui_kit::point(gpui_kit::px(width * 0.5), gpui_kit::px(height * 0.5)),
            cx,
        );
        let start = i64::from(content.read(cx).state_for_test().read(cx).fill_tolerance);
        window.press("right", cx);
        window.press("right", cx);
        window.press("up", cx);
        assert_eq!(
            i64::from(content.read(cx).state_for_test().read(cx).fill_tolerance),
            start + 3,
            "arrows step the value"
        );
        window.press("home", cx);
        assert_eq!(content.read(cx).state_for_test().read(cx).fill_tolerance, 0);
        window.press("end", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).fill_tolerance,
            255
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn paint_bucket_bar_presets_field_and_contiguous_are_live(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::PaintBucket, 1.0);
        window.render_frame(cx);
        assert!(
            window.try_find("option-live-fill-tolerance").is_none(),
            "the Paint Bucket option bar offers no live strip"
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).fill_tolerance,
            32
        );

        window.click("opt-tol-64", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).fill_tolerance,
            64
        );

        window.click("opt-tolerance", cx);
        window.press("1", cx);
        window.press("0", cx);
        window.press("0", cx);
        window.press("enter", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).fill_tolerance,
            100
        );

        assert!(content.read(cx).state_for_test().read(cx).fill_contiguous);
        window.click("opt-contiguous", cx);
        assert!(!content.read(cx).state_for_test().read(cx).fill_contiguous);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn eyedropper_bar_checkbox_toggles_auto_copy(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Eyedropper, 1.0);
        window.render_frame(cx);
        assert!(
            window
                .try_find("option-live-eyedropper-auto-copy")
                .is_none(),
            "the Eyedropper option bar offers no live strip"
        );
        assert!(!content.read(cx).state_for_test().read(cx).auto_copy_hex);
        window.click("opt-auto-copy", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).auto_copy_hex,
            "the chrome checkbox drives the setting"
        );
        window.click("opt-auto-copy", cx);
        assert!(!content.read(cx).state_for_test().read(cx).auto_copy_hex);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn eyedropper_hex_badge_shows_copy_tooltip(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Eyedropper, 1.0);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-copy-hex-tooltip").is_none(),
            "the HEX badge is tooltip-free until it is hovered"
        );
        window.hover("opt-copy-hex", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-copy-hex-tooltip").is_some(),
            "the HEX badge carries its click-to-copy tooltip"
        );
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-copy-hex-tooltip").is_none(),
            "leaving the badge hides the tooltip"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn text_size_num_field_commits_live_and_escape_reverts(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);

        window.click("opt-text-size", cx);
        window.press("7", cx);
        window.press("2", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).text_font_size,
            72.0,
            "valid keystrokes commit live"
        );

        window.click("opt-text-size", cx);
        window.press("9", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).text_font_size,
            9.0
        );
        window.press("escape", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).text_font_size,
            72.0,
            "Escape restores the value the edit started from"
        );

        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        tab_to_canvas(window, cx);
        window.press("a", cx);
        content.update(cx, |app, cx| {
            let _ = app.commit_text_edit(cx);
        });
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(data.font_size, 72.0, "the created layer inherits the value");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn text_option_controls_reach_the_created_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);
        window.click("opt-text-size", cx);
        window.press("5", cx);
        window.press("0", cx);
        window.press("enter", cx);
        window.click("opt-weight", cx);
        window.render_frame(cx);
        window.click("opt-weight-option-5", cx);
        window.click("opt-italic", cx);
        window.click("opt-align-center", cx);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        tab_to_canvas(window, cx);
        window.press("a", cx);
        content.update(cx, |app, cx| {
            let _ = app.commit_text_edit(cx);
        });
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(data.font_size, 50.0);
        assert_eq!(data.font_weight, 700.0);
        assert_eq!(
            data.font_style,
            photrez_core::document::layer_params::TextFontStyle::Italic
        );
        assert_eq!(
            data.align,
            photrez_core::document::layer_params::TextAlign::Center
        );
    })
    .expect("test window remains open");
}
