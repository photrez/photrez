//! The New Document dialog: the preset creates a dirty tab, the form types the name and the size.

use super::*;

#[gpui_kit::test]
fn new_document_dialog_creates_a_dirty_tab_with_the_chosen_preset(
    cx: &mut gpui_kit::TestAppContext,
) {
    use crate::state::NewDocCategory;

    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| app.open_new_document(cx));
        window.render_frame(cx);
        window.click("new-doc-tab-1", cx);
        window.render_frame(cx);
        {
            let form = content
                .read(cx)
                .new_document_form_for_test(cx)
                .expect("the dialog owns a form");
            assert_eq!(form.category, NewDocCategory::SocialMedia);
            assert_eq!(form.category.presets()[0].name, "Facebook Post");
        }
        window.click("new-doc-preset-0", cx);
        window.render_frame(cx);
        {
            let form = content
                .read(cx)
                .new_document_form_for_test(cx)
                .expect("the dialog owns a form");
            assert_eq!(form.name, "Facebook Post", "a preset sets the name");
            assert_eq!((form.width_px, form.height_px), (1200, 1200));
        }
        window.click("new-doc-create", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(state.active_dialog, None, "Create closes the dialog");
        assert!(state.new_document.is_none(), "the form is discarded");
        assert_eq!(state.tabs.len(), 2, "the new document opens in a tab");
        assert!(state.tabs[1].dirty, "a created document starts dirty");
        assert_eq!(
            state.tabs[1].name, "Facebook Post",
            "the tab carries the form's name"
        );
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((1200, 1200))
        );
        assert_eq!(
            content.read(cx).active_core_layer_count_for_test(),
            Some(1),
            "the app's new document is one Background layer"
        );
        assert!(
            content
                .read(cx)
                .active_document_id_for_test()
                .as_deref()
                .is_some_and(|id| id.starts_with("native-doc-"))
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn new_document_form_types_the_name_and_size_and_converts_units(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| app.open_new_document(cx));
        window.render_frame(cx);

        window.click("new-doc-name", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            press_char(app, window, cx, "space", " ");
            for key in ["A", "r", "t"] {
                press_key(app, window, cx, key);
            }
        });
        window.render_frame(cx);
        {
            let form = content
                .read(cx)
                .new_document_form_for_test(cx)
                .expect("the dialog owns a form");
            assert_eq!(form.name, "New Project Art");
        }

        window.click("new-doc-width", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            for _ in 0..4 {
                press_backspace(app, window, cx);
            }
            for key in ["5", "0", "0"] {
                press_key(app, window, cx, key);
            }
        });
        window.click("new-doc-height", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            for _ in 0..4 {
                press_backspace(app, window, cx);
            }
            for key in ["2", "0", "0"] {
                press_key(app, window, cx, key);
            }
        });
        window.render_frame(cx);

        window.click("new-doc-unit", cx);
        window.render_frame(cx);
        window.click("new-doc-unit-cm", cx);
        window.render_frame(cx);
        {
            let form = content
                .read(cx)
                .new_document_form_for_test(cx)
                .expect("the dialog owns a form");
            assert_eq!((form.width_px, form.height_px), (500, 200));
            assert_eq!(form.width_text, "13.23", "500 px formats as 13.23 cm");
            assert_eq!(form.height_text, "5.29", "200 px formats as 5.29 cm");
        }

        window.click("new-doc-create", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((500, 200))
        );
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(state.tabs[1].name, "New Project Art");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn new_document_transparent_background_starts_with_an_empty_layer(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| app.open_new_document(cx));
        window.render_frame(cx);
        window.click("new-doc-background", cx);
        window.render_frame(cx);
        window.click("new-doc-background-transparent", cx);
        window.render_frame(cx);
        window.click("new-doc-create", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_composite_pixel_for_test(0, 0),
            Some([0, 0, 0, 0]),
            "a transparent background leaves the layer empty"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn a_double_press_on_a_preset_creates_the_document(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| app.open_new_document(cx));
        window.render_frame(cx);
        window.double_click("new-doc-preset-1", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(
            state.active_dialog, None,
            "a double-click creates the document immediately"
        );
        assert_eq!(state.tabs.len(), 2);
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((2480, 3508)),
            "the double press uses the A4 preset"
        );
    })
    .expect("test window remains open");
}

/// Preset press -> repaint. Ignored by default because parallel test load
/// inflates the percentiles; run with `--ignored --exact`.
#[gpui_kit::test]
#[ignore = "perf measurement; run serially with --ignored --exact"]
fn new_document_preset_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| app.open_new_document(cx));
        window.render_frame(cx);
        let press = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App, index: usize| {
            let position = window
                .find(format("new-doc-preset-{index}"))
                .bounds()
                .center();
            let _ = window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position,
                    ..Default::default()
                }),
                cx,
            );
            let _ = window.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position,
                    ..Default::default()
                }),
                cx,
            );
        };
        // One unmeasured warm-up press.
        press(window, cx, 1);
        window.render_frame(cx);

        let mut samples = Vec::new();
        for sample in 0..9 {
            let index = sample % 2;
            let expected = if index == 0 { "A3" } else { "A4" };
            let start = std::time::Instant::now();
            press(window, cx, index);
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(
                content
                    .read(cx)
                    .new_document_form_for_test(cx)
                    .map(|form| form.name),
                Some(expected.to_string()),
                "sample {sample} applies the preset"
            );
        }
        let mut sorted = samples.clone();
        sorted.sort_by(f64::total_cmp);
        let p50 = sorted[sorted.len() / 2];
        let p95 = sorted[(sorted.len() as f64 * 0.95).ceil() as usize - 1];
        eprintln!("NEW_DOC_MS samples={samples:?} p50={p50:.3} p95={p95:.3}");
        assert!(
            p95 <= 33.3,
            "preset press p95 {p95:.3} ms exceeds the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn escape_closes_an_open_dropdown_before_the_dialog(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| app.open_new_document(cx));
        window.render_frame(cx);
        window.click("new-doc-unit", cx);
        window.render_frame(cx);
        assert!(
            content
                .read(cx)
                .new_document_form_for_test(cx)
                .is_some_and(|form| form.unit_open),
            "the unit select opens"
        );

        window.press("escape", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(
                state.active_dialog,
                Some(crate::state::DialogId::NewDocument),
                "the first Escape only closes the dropdown"
            );
            assert_eq!(
                state.new_document.as_ref().map(|form| form.unit_open),
                Some(false)
            );
        }

        window.press("escape", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(
            state.active_dialog, None,
            "the second Escape closes the dialog"
        );
        assert!(state.new_document.is_none());
    })
    .expect("test window remains open");
}
