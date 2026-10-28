use super::input::{drag_doc, mount_archive_app, pixels, select_layer, set_tool, tab_to_canvas};
use super::*;
use crate::state::{ShapeTool, ToolId};
use gpui_kit::test::TestWindowExt as _;
use photrez_core::ptz::content::{LayerType, SelectionShape, SelectionState};
use photrez_core::ptz::layer_params::ShapeKind;

fn set_state(
    content: &Entity<App>,
    cx: &mut gpui_kit::App,
    update: impl FnOnce(&mut crate::state::AppState),
) {
    content.update(cx, |app, cx| {
        app.state_for_test().update(cx, |state, _| update(state));
    });
}

#[gpui_kit::test]
fn brush_opacity_scales_the_stroke_alpha(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 1.0);
        set_state(&content, cx, |state| state.brush_opacity = 0.5);
        window.render_frame(cx);
        let (w, h, before) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (1.5, 0.5), cx);
        let (_, _, after) = pixels(&content, cx);
        assert_ne!(after, before, "the half-opacity stroke still paints");
        // Black at 50% over the opaque fixture leaves a lighter pixel than a
        // full-opacity stroke would (0 instead of the channel value).
        assert!(
            after[0] > 0 && after[0] < before[0],
            "opacity 0.5 must blend, before={before:?} after={after:?}"
        );
        assert_eq!(after[3], 255, "the opaque fixture keeps its alpha");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn bucket_tolerance_and_contiguity_choose_the_filled_region(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::PaintBucket, 1.0);
        window.render_frame(cx);
        let (w, h, before) = pixels(&content, cx);

        // Exact tolerance, non-contiguous: only the equal-coloured pixel 0.
        set_state(&content, cx, |state| {
            state.fill_tolerance = 0;
            state.fill_contiguous = false;
        });
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        let (_, _, exact) = pixels(&content, cx);
        assert_eq!(&exact[0..4], &[0, 0, 0, 255], "pixel 0 is filled");
        assert_eq!(&exact[4..8], &before[4..8], "pixel 1 keeps its colour");

        content.update(cx, |app, cx| {
            app.step_history(true, cx)
                .expect("undo restores the fixture");
        });
        assert_eq!(pixels(&content, cx).2, before);

        // Full tolerance, non-contiguous: every pixel is in range.
        set_state(&content, cx, |state| {
            state.fill_tolerance = 255;
            state.fill_contiguous = false;
        });
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        let (_, _, all) = pixels(&content, cx);
        assert_eq!(&all[4..8], &[0, 0, 0, 255], "pixel 1 joins the fill");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn bucket_honours_the_pixel_selection_mask(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::PaintBucket, 1.0);
        set_state(&content, cx, |state| state.fill_tolerance = 255);
        content.update(cx, |app, cx| {
            app.set_core_selection_for_test(
                SelectionState {
                    x: 0.,
                    y: 0.,
                    width: 1.,
                    height: 1.,
                    angle: 0.,
                    shape: Some(SelectionShape::Rect),
                    inverted: None,
                },
                cx,
            )
            .expect("selection is accepted");
        });
        window.render_frame(cx);
        let (w, h, before) = pixels(&content, cx);
        // Clicking outside the mask must not spill into pixel 1.
        drag_doc(window, (w, h), (1.5, 0.5), (1.5, 0.5), cx);
        let (_, _, after) = pixels(&content, cx);
        assert_eq!(
            &after[4..8],
            &before[4..8],
            "the mask keeps the unselected pixel"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn shape_kind_option_creates_an_ellipse_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Shape, 1.0);
        set_state(&content, cx, |state| state.shape_kind = ShapeTool::Ellipse);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.1, 0.1), (0.9, 0.9), cx);
        assert_eq!(
            content.read(cx).last_shape_kind_for_test(),
            Some(ShapeKind::Ellipse),
            "the shape option must reach the created layer"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn eyedropper_samples_the_composite_not_the_selected_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (rows, _) = super::input::layer_rows(&content, cx);
        let bottom = rows
            .iter()
            .find(|row| row.core_id.as_deref() == Some("bottom"))
            .expect("the bottom row exists")
            .id;
        select_layer(&content, cx, bottom);
        set_tool(&content, cx, ToolId::Eyedropper, 1.0);
        window.render_frame(cx);
        // Pixel (0,0) is blue on the top layer, red on the selected bottom.
        drag_doc(window, (2, 1), (0.5, 0.5), (0.5, 0.5), cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).fg_color,
            [0, 0, 255, 255],
            "the eyedropper reads the visible composite"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn option_bar_steppers_change_the_live_tool_settings(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 1.0);
        window.render_frame(cx);
        window.click("option-live-brush-opacity-dec", cx);
        window.render_frame(cx);
        let opacity = content.read(cx).state_for_test().read(cx).brush_opacity;
        assert!((opacity - 0.9).abs() < 1e-6, "opacity={opacity}");
        window.click("option-live-brush-smoothing-inc", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).brush_smoothing,
            5.0
        );

        set_tool(&content, cx, ToolId::PaintBucket, 1.0);
        window.render_frame(cx);
        window.click("option-live-fill-tolerance-inc", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).fill_tolerance,
            36
        );

        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);
        window.click("option-live-shape-kind", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).shape_kind,
            ShapeTool::Ellipse
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn text_click_opens_an_editor_and_typing_commits_to_the_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        assert!(
            content.read(cx).text_edit_content_for_test(cx).is_some(),
            "the text click opens the editor"
        );
        assert!(
            window.try_find("text-editor").is_some(),
            "the in-place editor overlay is mounted"
        );

        tab_to_canvas(window, cx);
        window.press("h", cx);
        window.press("i", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).text_edit_content_for_test(cx).as_deref(),
            Some("Texthi")
        );
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).text_edit_content_for_test(cx).is_none(),
            "Escape closes the editor"
        );
        let text_id = content
            .read(cx)
            .text_layer_id_for_test()
            .expect("the text layer exists");
        assert_eq!(
            content
                .read(cx)
                .text_layer_content_for_test(&text_id)
                .as_deref(),
            Some("Texthi"),
            "the typed text is committed to the layer"
        );
        assert!(content
            .read(cx)
            .active_document_save_state_for_test()
            .is_some_and(|(_, _, dirty)| dirty));
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn mounted_shape_saves_and_reloads_as_a_shape_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.1, 0.1), (0.9, 0.9), cx);
        let saved = directory.path().join("shape-saved.ptz");
        content
            .update(cx, |app, cx| app.save_to_path_for_test(&saved, cx))
            .expect("shape document saves");
        let file_io = LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
            .expect("file boundary");
        file_io.approve_path(&saved).expect("saved path approved");
        content
            .update(cx, |app, cx| {
                app.open_startup_document_for_test(&saved, &file_io, cx)
            })
            .expect("saved document reopens");
        let types = content
            .read(cx)
            .active_core_layer_types_for_test()
            .expect("layers exist");
        assert!(
            types
                .iter()
                .any(|kind| matches!(kind, photrez_core::ptz::content::LayerType::Shape)),
            "the shape layer survives save/reload: {types:?}"
        );
        let _ = source;
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn mounted_text_renders_into_the_exported_png(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(160, 96, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Text;
                state.fg_color = [0, 0, 0, 255];
            });
        });
        window.render_frame(cx);
        drag_doc(window, (160, 96), (12.0, 12.0), (12.0, 12.0), cx);
        tab_to_canvas(window, cx);
        window.press("a", cx);
        window.press("g", cx);
        window.press("escape", cx);
        window.render_frame(cx);

        let exported = directory.path().join("text-export.png");
        content
            .update(cx, |app, cx| app.export_to_path_for_test(&exported, cx))
            .expect("text document exports");
        let image = ::image::open(&exported)
            .expect("exported PNG opens")
            .to_rgba8();
        let ink = image
            .pixels()
            .filter(|pixel| pixel[0] < 100 && pixel[1] < 100 && pixel[2] < 100)
            .count();
        assert!(ink > 0, "the exported PNG contains text ink");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn mounted_shape_renders_into_the_exported_png(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(96, 96, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Shape;
                state.fg_color = [0, 0, 0, 255];
                state.shape_kind = crate::state::ShapeTool::Rect;
            });
        });
        window.render_frame(cx);
        drag_doc(window, (96, 96), (16.0, 16.0), (80.0, 80.0), cx);
        let exported = directory.path().join("shape-export.png");
        content
            .update(cx, |app, cx| app.export_to_path_for_test(&exported, cx))
            .expect("shape document exports");
        let image = ::image::open(&exported)
            .expect("exported PNG opens")
            .to_rgba8();
        let ink = image
            .pixels()
            .filter(|pixel| pixel[0] < 100 && pixel[1] < 100 && pixel[2] < 100)
            .count();
        assert!(ink > 100, "the exported PNG contains the shape");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn brush_preset_button_cycles_the_frozen_presets(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 1.0);
        window.render_frame(cx);
        window.click("option-live-brush-preset", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.brush_size, 40.0);
            assert!((state.brush_hardness - 0.15).abs() < 1e-6);
        }
        window.click("option-live-brush-preset", cx);
        window.render_frame(cx);
        assert_eq!(content.read(cx).state_for_test().read(cx).brush_size, 5.0);

        set_tool(&content, cx, ToolId::Eraser, 1.0);
        window.render_frame(cx);
        window.click("option-live-brush-preset", cx);
        window.render_frame(cx);
        assert_eq!(content.read(cx).state_for_test().read(cx).eraser_size, 30.0);
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
        window.click("option-live-text-size-inc", cx);
        window.click("option-live-text-bold", cx);
        window.click("option-live-text-italic", cx);
        window.click("option-live-text-align", cx);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
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
            photrez_core::ptz::layer_params::TextFontStyle::Italic
        );
        assert_eq!(
            data.align,
            photrez_core::ptz::layer_params::TextAlign::Center
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn adjustment_steppers_brighten_the_composite_and_undo(cx: &mut gpui_kit::TestAppContext) {
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
        window.click("adj-0-inc", cx);
        let cursor_one = content.read(cx).active_core_history_cursor_for_test();
        window.click("adj-0-inc", cx);
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
            10.0
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
                5.0,
                "one undo restores +5"
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
fn properties_panel_text_typography_edits_and_rasterize(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        content.update(cx, |app, cx| {
            let _ = app.commit_text_edit(cx);
        });
        window.render_frame(cx);
        assert!(
            window.try_find("prop-text-size-inc").is_some(),
            "a text selection exposes the typography controls"
        );

        window.click("prop-text-size-inc", cx);
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(data.font_size, 50.0, "+2px on the default 48px");

        window.click("prop-text-bold", cx);
        window.click("prop-text-italic", cx);
        window.click("prop-text-align", cx);
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(data.font_weight, 700.0);
        assert_eq!(
            data.font_style,
            photrez_core::ptz::layer_params::TextFontStyle::Italic
        );
        assert_eq!(
            data.align,
            photrez_core::ptz::layer_params::TextAlign::Center
        );

        window.click("prop-rasterize", cx);
        window.render_frame(cx);
        let layer_type = content
            .update(cx, |app, cx| {
                app.selected_layer_for_cursor(cx)
                    .map(|(_, layer)| layer.layer_type)
            })
            .expect("selected layer");
        assert_eq!(
            layer_type,
            photrez_core::ptz::content::LayerType::Raster,
            "rasterize converts the parametric layer"
        );
        assert!(
            window.try_find("prop-text-size-inc").is_none(),
            "the typography controls disappear once the layer is raster"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn properties_panel_shows_no_image_placeholder_without_a_document(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let doc_id = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .document_id
            .clone()
            .expect("fixture document id");
        window.click(format!("close-tab-{doc_id}"), cx);
        window.render_frame(cx);
        assert!(
            !content.read(cx).state_for_test().read(cx).has_document,
            "closing the last tab leaves no document"
        );
        assert!(
            window.try_find("properties-no-image").is_some(),
            "the Properties panel shows the no-image placeholder"
        );
        assert!(
            window.try_find("prop-flip-h").is_none(),
            "no layer actions without a document"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn properties_panel_falls_back_to_canvas_properties(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(120, 80, cx).expect("blank document");
            app.delete_selected_layer(cx);
        });
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layers.len(),
            0,
            "the last layer is gone"
        );
        assert!(
            window.try_find("canvas-size").is_some(),
            "the panel falls back to Canvas Properties"
        );
        window.click("canvas-size", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            Some(crate::state::DialogId::ResizeCanvas),
            "the size field opens the Resize Canvas dialog"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn properties_panel_multi_selection_distributes_and_aligns(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(100, 100, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);
        drag_doc(window, (100, 100), (0.0, 10.0), (50.0, 60.0), cx);
        drag_doc(window, (100, 100), (30.0, 10.0), (80.0, 60.0), cx);
        drag_doc(window, (100, 100), (70.0, 10.0), (100.0, 60.0), cx);

        content.update(cx, |app, cx| {
            // Shape layers carry the document size at creation, so widen the
            // canvas afterwards to give align/distribute room to move.
            app.state_for_test().update(cx, |state, _| {
                state.resize_canvas = Some(crate::state::ResizeSession {
                    width: "500".to_string(),
                    height: "100".to_string(),
                    field: 0,
                });
            });
            app.apply_resize_canvas(cx).expect("canvas resizes");
            for (name, delta) in [
                ("Shape at 0,10", 0.0),
                ("Shape at 30,10", 90.0),
                ("Shape at 70,10", 230.0),
            ] {
                let row = {
                    let state = app.state_for_test().read(cx);
                    state
                        .layers
                        .iter()
                        .find(|row| row.name == name)
                        .map(|row| row.id)
                        .expect("shape row")
                };
                app.state_for_test().update(cx, |state, _| {
                    state.selected_layer = row;
                    state.selected_layers = vec![row];
                });
                app.project_layers(cx);
                if delta != 0.0 {
                    app.nudge_selected_layer(delta, 0.0, cx)
                        .expect("nudge applies");
                }
            }
            let rows: Vec<u64> = {
                let state = app.state_for_test().read(cx);
                state
                    .layers
                    .iter()
                    .filter(|row| row.name.starts_with("Shape at"))
                    .map(|row| row.id)
                    .collect()
            };
            assert_eq!(rows.len(), 3);
            app.state_for_test().update(cx, |state, _| {
                state.selected_layer = rows[0];
                state.selected_layers = rows;
            });
            app.project_layers(cx);
        });
        window.render_frame(cx);
        assert!(
            window.try_find("prop-align-left").is_some(),
            "the multi-selection inspector mounts"
        );

        window.click("prop-distribute-h", cx);
        let mut xs: Vec<f64> = content
            .read(cx)
            .core_layer_origins_for_test(LayerType::Shape)
            .into_iter()
            .map(|origin| origin.0)
            .collect();
        xs.sort_by(f64::total_cmp);
        for (actual, expected) in xs.iter().zip([0.0, 150.0, 300.0]) {
            assert!(
                (actual - expected).abs() < 1e-2,
                "free space is split evenly: {xs:?}"
            );
        }

        window.click("prop-align-center-h", cx);
        let xs = content
            .read(cx)
            .core_layer_origins_for_test(LayerType::Shape);
        assert!(
            xs.iter().all(|origin| (origin.0 - 200.0).abs() < 1e-6),
            "centre alignment lands every shape at (500 - 100) / 2: {xs:?}"
        );
    })
    .expect("test window remains open");
}
