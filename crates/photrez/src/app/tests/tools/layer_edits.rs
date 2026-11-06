//! Edits that reach the layer: adjustments, panel actions, the text option bar,
//! rasterize and the text stroke popover.

use super::*;

#[gpui_kit::test]
fn adjustment_sliders_brighten_the_composite_and_undo(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.inspector_tab = crate::state::InspectorTab::Adjustments;
            });
        });
        window.render_frame(cx);
        let before = content
            .read(cx)
            .active_composite_pixel_for_test(0, 0)
            .expect("composite pixel");
        let track = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .adjustment_slider_bounds[0]
            .get()
            .expect("the Bright track is laid out");
        let mut press = |fraction: f32, cx: &mut gpui_kit::App| {
            let x = track.origin.x + gpui_kit::px(f32::from(track.size.width) * fraction);
            let y = track.origin.y + track.size.height / 2.0;
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                    button: gpui_kit::MouseButton::Left,
                    position: gpui_kit::point(x, y),
                    ..Default::default()
                }),
                cx,
            );
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                    button: gpui_kit::MouseButton::Left,
                    position: gpui_kit::point(x, y),
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
        };
        press(0.55, cx);
        let cursor_one = content.read(cx).active_core_history_cursor_for_test();
        press(0.60, cx);
        let cursor_two = content.read(cx).active_core_history_cursor_for_test();
        window.render_frame(cx);
        assert_eq!((cursor_one, cursor_two), (Some(1), Some(2)));
        let after = content
            .read(cx)
            .active_composite_pixel_for_test(0, 0)
            .expect("composite pixel");
        assert!(
            after[0] > before[0] && after[1] > before[1] && after[2] > before[2],
            "brightness must lighten the composite: {before:?} -> {after:?}"
        );
        assert_eq!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .adjustment_brightness,
            20.0
        );
        content.update(cx, |app, cx| {
            let stepped = app.step_history(true, cx).expect("undo one");
            assert!(stepped);
            assert_eq!(app.active_core_history_cursor_for_test(), Some(1));
            let (_, layer) = app.selected_layer_for_cursor(cx).unwrap();
            assert_eq!(
                layer
                    .basic_adjustment
                    .as_ref()
                    .map(|adjustment| adjustment.brightness)
                    .unwrap_or(0.0),
                10.0,
                "one undo restores +10"
            );
            let stepped = app.step_history(true, cx).expect("undo two");
            assert!(stepped);
            assert_eq!(app.active_core_history_cursor_for_test(), Some(0));
            let (_, layer) = app.selected_layer_for_cursor(cx).unwrap();
            assert_eq!(
                layer
                    .basic_adjustment
                    .as_ref()
                    .map(|adjustment| adjustment.brightness),
                None,
                "second undo clears the adjustment"
            );
        });
        assert_eq!(
            content
                .read(cx)
                .active_composite_pixel_for_test(0, 0)
                .expect("composite pixel"),
            before
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn properties_panel_actions_reach_the_selected_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("prop-flip-h").is_some(),
            "the properties panel mounts its live actions"
        );

        let flip = |cx: &mut gpui_kit::App| {
            content
                .update(cx, |app, cx| {
                    app.selected_layer_for_cursor(cx)
                        .map(|(_, layer)| layer.transform.flip_h)
                })
                .expect("selected layer")
        };
        assert!(!flip(cx));
        window.click("prop-flip-h", cx);
        assert!(flip(cx), "flip H toggles the layer transform");
        window.click("prop-flip-h", cx);
        assert!(!flip(cx), "flip H toggles back");

        content.update(cx, |app, cx| {
            app.nudge_selected_layer(12.0, -7.0, cx)
                .expect("nudge applies");
        });
        window.render_frame(cx);
        assert_eq!(
            content
                .read(cx)
                .selected_transform_for_test(cx)
                .map(|transform| (transform.0, transform.1)),
            Some((12.0, -7.0))
        );
        window.click("prop-reset", cx);
        assert_eq!(
            content
                .read(cx)
                .selected_transform_for_test(cx)
                .map(|transform| (transform.0, transform.1, transform.4)),
            Some((0.0, 0.0, 0.0)),
            "reset returns the layer to its origin"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn text_option_bar_edits_the_selected_layer_and_syncs_defaults(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        tab_to_canvas(window, cx);
        window.press("a", cx);
        content.update(cx, |app, cx| {
            let _ = app.commit_text_edit(cx);
        });
        window.render_frame(cx);
        assert!(
            window.try_find("opt-text-size").is_some(),
            "a text selection exposes the typography controls"
        );
        assert!(
            window.try_find("opt-text-rasterize").is_some(),
            "layer-only actions appear in edit mode"
        );

        window.click("opt-text-size", cx);
        window.press("5", cx);
        window.press("0", cx);
        window.press("enter", cx);
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(data.font_size, 50.0, "typed 50 replaces the resting 48");
        assert_eq!(
            content.read(cx).state_for_test().read(cx).text_font_size,
            50.0,
            "editing the layer syncs the next-text default"
        );

        window.click("opt-weight", cx);
        window.render_frame(cx);
        window.click("opt-weight-option-5", cx);
        window.click("opt-italic", cx);
        window.click("opt-underline", cx);
        window.click("opt-strikethrough", cx);
        window.click("opt-uppercase", cx);
        window.click("opt-align-center", cx);
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(data.font_weight, 700.0);
        assert_eq!(
            data.font_style,
            photrez_core::document::layer_params::TextFontStyle::Italic
        );
        assert_eq!(
            data.align,
            photrez_core::document::layer_params::TextAlign::Center
        );
        assert_eq!(data.underline, Some(true));
        assert_eq!(data.strikethrough, Some(true));
        assert_eq!(data.uppercase, Some(true));
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.text_font_weight, 700.0);
            assert!(state.text_italic);
            assert_eq!(state.text_align, crate::state::TextAlignTool::Center);
            assert!(state.text_underline);
            assert!(state.text_strikethrough);
            assert!(state.text_uppercase);
        }

        // The host renders the text bitmap and hands it to RasterizeLayer,
        // which materializes it as the rasterized layer's canonical buffer.
        // A layer whose features the core rasterizer cannot render (the
        // decorated case: underline/strikethrough/uppercase/weight) refuses
        // the rasterize with a typed error instead of producing a Raster
        // layer the compositor cannot paint. The pre-fix failure was that both
        // kinds rasterized metadata-only and failed with MissingPixels.)
        content.update(cx, |app, cx| {
            let error = app
                .rasterize_selected_layer(cx)
                .expect_err("a decorated text layer refuses the rasterize");
            assert!(
                error.contains("cannot be rendered"),
                "typed refusal, got {error}"
            );
        });
        window.render_frame(cx);
        let layer_type = content
            .update(cx, |app, cx| {
                app.selected_layer_for_cursor(cx)
                    .map(|(_, layer)| layer.layer_type)
            })
            .expect("selected layer");
        assert_eq!(
            layer_type,
            photrez_core::document::content::LayerType::Text,
            "the refused rasterize leaves the layer parametric"
        );
        // Not proven here: that the option-bar trigger reports this refusal. The
        // trigger is present but not visible in this fixture once the decoration
        // flags are set, so a click cannot reach it. The handler now forwards
        // through `report_layer_op_for_view`, the same path every sibling layer
        // action uses; a fixture where the trigger is clickable is the check that
        // proves the reporting end to end.
    })
    .expect("test window remains open");
}

/// A supported text layer rasterizes with its host-rendered
/// bitmap materialized as the canonical buffer, and the document composites.
#[gpui_kit::test]
fn text_rasterize_materializes_pixels_and_composites(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        tab_to_canvas(window, cx);
        window.press("a", cx);
        content.update(cx, |app, cx| {
            let _ = app.commit_text_edit(cx);
        });
        window.render_frame(cx);

        content.update(cx, |app, cx| {
            app.rasterize_selected_layer(cx)
                .expect("a supported text layer rasterizes");
        });
        window.render_frame(cx);
        let (layer_type, store_pixels) = content.update(cx, |app, cx| {
            let (_, layer) = app.selected_layer_for_cursor(cx).expect("selected layer");
            let layer_type = layer.layer_type;
            let pixels = app
                .active_document
                .as_ref()
                .and_then(|doc| app.sessions.get(&doc.doc_id))
                .and_then(|session| {
                    let loaded = session.loaded_document();
                    loaded
                        .content_snapshot()
                        .and_then(|c| c.layers.iter().find(|l| l.id == layer.id).cloned())
                        .and_then(|l| {
                            loaded
                                .registry
                                .get_layer(loaded.document_id(), &l.id)
                                .map(|px| (px.width, px.height, px.pixels.len(), l.width, l.height))
                        })
                });
            (layer_type, pixels)
        });
        assert_eq!(
            layer_type,
            photrez_core::document::content::LayerType::Raster,
            "rasterize converts the parametric layer"
        );
        let (width, height, len, layer_w, layer_h) =
            store_pixels.expect("the rasterized layer carries pixels");
        assert_eq!(
            len,
            width as usize * height as usize * 4,
            "the materialized buffer matches its dims"
        );
        assert_eq!(
            (f64::from(width), f64::from(height)),
            (layer_w, layer_h),
            "the layer frame matches the materialized bitmap"
        );
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .status_error
                .is_none(),
            "a successful rasterize leaves no status error"
        );
    })
    .expect("test window remains open");
}

/// Perf: one keystroke in the Text bar's size field commits live and
/// paints a frame. The key is dispatched directly; gpui-kit's press helper
/// renders a frame before and after and would charge three frames.
#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn text_bar_size_commit_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        tab_to_canvas(window, cx);
        window.press("a", cx);
        content.update(cx, |app, cx| {
            let _ = app.commit_text_edit(cx);
        });
        window.render_frame(cx);
        let mut samples = Vec::new();
        let mut last: Option<f64> = None;
        for i in 0..9 {
            window.click("opt-text-size", cx);
            window.render_frame(cx);
            let digit = if i % 2 == 0 { "6" } else { "7" };
            let start = std::time::Instant::now();
            let _ = window.dispatch_event(
                PlatformInput::KeyDown(gpui_kit::KeyDownEvent {
                    keystroke: gpui_kit::Keystroke {
                        modifiers: Default::default(),
                        key: digit.to_string(),
                        key_char: Some(digit.to_string()),
                    },
                    is_held: false,
                    prefer_character_input: false,
                }),
                cx,
            );
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            window.press("enter", cx);
            let size = content
                .read(cx)
                .text_layer_data_for_test()
                .expect("text data")
                .font_size;
            if let Some(previous) = last {
                assert_ne!(size, previous, "each typed digit commits a new size");
            }
            last = Some(size);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "text_bar_size p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "text bar size commit p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn text_stroke_popover_edits_the_layer_and_the_next_text_defaults(
    cx: &mut gpui_kit::TestAppContext,
) {
    use photrez_core::document::layer_params::TextStrokeAlign;

    let directory = TestDirectory::new();
    // The 1920x1080 client at 125% scaling gives a 1536 px layout width: the
    // only width at which the option-bar geometry lays every text control in
    // reach.
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1536.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);

        // Draw mode: the popover drives the next-text defaults.
        assert!(
            window.try_find("opt-text-stroke").is_some(),
            "the text bar exposes the stroke pill"
        );
        assert!(
            window.try_find("opt-text-stroke-popover").is_none(),
            "the popover starts closed"
        );
        window.click("opt-text-stroke", cx);
        window.render_frame(cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .text_stroke_popover_open,
            "the pill opens the popover"
        );
        assert!(
            window.try_find("opt-text-stroke-popover").is_some(),
            "the pill mounts the popover"
        );
        window.click("opt-text-stroke-toggle", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.text_stroke_width, 4.0, "the switch enables at 4");
            assert_eq!(
                state.text_stroke_align,
                TextStrokeAlign::Outside,
                "position starts outside"
            );
        }
        window.click("opt-text-stroke", cx);
        window.render_frame(cx);

        // The created layer inherits the popover's defaults. The session starts
        // empty, so type one character before committing.
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        tab_to_canvas(window, cx);
        window.press("a", cx);
        content.update(cx, |app, cx| {
            let _ = app.commit_text_edit(cx);
        });
        window.render_frame(cx);
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(data.stroke.width, 4.0, "the new layer carries the width");
        assert_eq!(data.stroke.color, "#000000", "and the default colour");
        assert_eq!(data.stroke.align, Some(TextStrokeAlign::Outside));

        // Edit mode: the popover edits the selected layer.
        window.click("opt-text-stroke", cx);
        window.render_frame(cx);
        window.click("opt-text-stroke-width", cx);
        for digit in ["9", "9", "9"] {
            window.press(digit, cx);
        }
        window.press("enter", cx);
        window.render_frame(cx);
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(data.stroke.width, 100.0, "width clamps to 100");
        assert_eq!(
            content.read(cx).state_for_test().read(cx).text_stroke_width,
            100.0,
            "editing the layer syncs the next-text width"
        );

        window.click("opt-text-stroke-width", cx);
        window.press("0", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(data.stroke.width, 1.0, "the field floors at 1, not 0");

        window.click("opt-text-stroke-inc", cx);
        window.render_frame(cx);
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(data.stroke.width, 2.0, "the stepper adds one");

        let bounds = window.find("opt-text-stroke-slider").bounds();
        let width = f32::from(bounds.size.width);
        let height = f32::from(bounds.size.height);
        assert!(width > 0.0 && height > 0.0, "the track is laid out");
        window.drag(
            bounds.origin + gpui_kit::point(gpui_kit::px(width * 0.25), gpui_kit::px(height * 0.5)),
            bounds.origin + gpui_kit::point(gpui_kit::px(width * 0.75), gpui_kit::px(height * 0.5)),
            cx,
        );
        window.render_frame(cx);
        let value = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists")
            .stroke
            .width;
        assert!(
            (value - 75.25).abs() <= 3.0,
            "dragging to 75% maps to the 1..100 track, got {value}"
        );

        window.click("opt-text-stroke-color", cx);
        window.render_frame(cx);
        assert!(
            window.find("color-picker").visible(),
            "the colour row opens the picker"
        );
        content.update(cx, |app, cx| {
            assert_eq!(
                app.state_for_test()
                    .read(cx)
                    .color_picker
                    .as_ref()
                    .map(|session| session.target),
                Some(crate::state::ColorTarget::TextStroke),
                "the picker edits the text stroke target"
            );
        });
        for digit in ["1", "2", "3", "4", "5", "6"] {
            window.press(digit, cx);
        }
        window.press("enter", cx);
        window.render_frame(cx);
        {
            let data = content
                .read(cx)
                .text_layer_data_for_test()
                .expect("text data exists");
            assert_eq!(data.stroke.color, "#123456");
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.text_stroke_color, [0x12, 0x34, 0x56, 255]);
            assert_eq!(
                state.selected_text_stroke_color,
                Some([0x12, 0x34, 0x56, 255])
            );
        }

        window.click("opt-text-stroke-inside", cx);
        window.render_frame(cx);
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(data.stroke.align, Some(TextStrokeAlign::Inside));
        assert_eq!(
            content.read(cx).state_for_test().read(cx).text_stroke_align,
            TextStrokeAlign::Inside
        );

        content.update(cx, |app, cx| {
            assert!(app.step_history(true, cx).expect("undo the position"));
        });
        window.render_frame(cx);
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(
            data.stroke.align,
            Some(TextStrokeAlign::Outside),
            "undo restores the previous position"
        );

        window.press("escape", cx);
        assert!(
            !content
                .read(cx)
                .state_for_test()
                .read(cx)
                .text_stroke_popover_open,
            "Escape closes the popover"
        );
    })
    .expect("test window remains open");
}
