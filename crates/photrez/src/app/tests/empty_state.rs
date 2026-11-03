use super::*;
use gpui_kit::test::TestWindowExt as _;

/// Put the mounted app in the no-document state the workspace reaches when the
/// last tab closes (`App::activate_document`'s empty neighbour).
fn show_empty_workspace(app: &Entity<App>, cx: &mut gpui_kit::App) {
    app.update(cx, |app, cx| {
        app.state_for_test().update(cx, |state, _| {
            state.has_document = false;
            state.document_id = None;
            state.document_name.clear();
            state.layers.clear();
            state.history.clear();
            state.active_dialog = None;
        });
    });
}

#[gpui_kit::test]
fn fresh_start_without_open_path_mounts_empty_workspace(cx: &mut gpui_kit::TestAppContext) {
    let (window, content) = mount_app(cx, None, None);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);

        {
            let state = content.read(cx).state_for_test();
            let state = state.read(cx);
            assert!(!state.has_document, "a fresh launch has no document");
            assert_ne!(state.document_name, "Baseline");
            assert!(
                state.layers.is_empty(),
                "a fresh launch has no layer projection"
            );
        }

        let bounds = window.find("empty-open-image").bounds();
        assert!(
            f32::from(bounds.size.width) > 0.0 && f32::from(bounds.size.height) > 0.0,
            "the production open-image action is mounted"
        );
    })
    .expect("test window remains open");
}

/// The card's Quick Preset tiles create the document the tile names, at the
/// size printed under its label.
#[gpui_kit::test]
fn empty_workspace_preset_creates_the_named_document(cx: &mut gpui_kit::TestAppContext) {
    let (window, content) = mount_app(cx, None, None);
    cx.update_window(window.into(), |_, window, cx| {
        show_empty_workspace(&content, cx);
        window.render_frame(cx);

        window.click("empty-preset-3", cx);
        window.render_frame(cx);

        let state = content.read(cx).state_for_test();
        {
            let state = state.read(cx);
            assert!(state.has_document, "the preset opens a document");
            assert_eq!(state.document_name, "A4 Print (300 DPI)");
            assert_eq!(state.active_dialog, None);
            assert_eq!(state.tabs.len(), 1);
            assert_eq!(state.tabs[0].name, "A4 Print (300 DPI)");
        }
        let active = content
            .read(cx)
            .active_tab_id_for_test()
            .expect("the preset document is active");
        assert_eq!(state.read(cx).active_tab.as_deref(), Some(active.as_str()));
    })
    .expect("test window remains open");
}

/// New Document raises the production dialog instead of creating a document.
#[gpui_kit::test]
fn empty_workspace_new_document_opens_the_form(cx: &mut gpui_kit::TestAppContext) {
    let (window, content) = mount_app(cx, None, None);
    cx.update_window(window.into(), |_, window, cx| {
        show_empty_workspace(&content, cx);
        window.render_frame(cx);

        window.click("empty-new-document", cx);
        window.render_frame(cx);

        let state = content.read(cx).state_for_test();
        {
            let state = state.read(cx);
            assert_eq!(
                state.active_dialog,
                Some(crate::state::DialogId::NewDocument)
            );
            assert!(!state.has_document, "the form does not create anything yet");
        }
    })
    .expect("test window remains open");
}

/// Both action buttons are mounted with their production ids, so a click lands
/// on a registered hit box rather than on the card.
#[gpui_kit::test]
fn empty_workspace_mounts_both_action_buttons(cx: &mut gpui_kit::TestAppContext) {
    let (window, content) = mount_app(cx, None, None);
    cx.update_window(window.into(), |_, window, cx| {
        show_empty_workspace(&content, cx);
        window.render_frame(cx);
        for id in ["empty-open-image", "empty-new-document"] {
            let bounds = window.find(id).bounds();
            assert!(
                f32::from(bounds.size.width) > 0.0 && f32::from(bounds.size.height) > 0.0,
                "{id} is laid out"
            );
        }
        for id in [
            "empty-preset-0",
            "empty-preset-1",
            "empty-preset-2",
            "empty-preset-3",
        ] {
            window.find(id);
        }
    })
    .expect("test window remains open");
}

/// Closing the last document returns the shell to the empty workspace and
/// drops the closed document's panel projection with it.
#[gpui_kit::test]
fn closing_the_last_document_clears_the_document_projection(cx: &mut gpui_kit::TestAppContext) {
    let (window, content) = mount_app(cx, None, None);
    cx.update_window(window.into(), |_, window, cx| {
        show_empty_workspace(&content, cx);
        window.render_frame(cx);
        window.click("empty-preset-0", cx);
        window.render_frame(cx);

        window.press("ctrl-w", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            Some(crate::state::DialogId::DirtyClose)
        );

        window.click("dirty-close-discard", cx);
        window.render_frame(cx);

        {
            let state = content.read(cx).state_for_test();
            let state = state.read(cx);
            assert!(!state.has_document);
            assert!(
                state.layers.is_empty(),
                "the closed document's layers stay off the panel"
            );
            assert!(state.history.is_empty());
            assert!(state.tabs.is_empty());
        }
        window.find("empty-preset-0");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn empty_workspace_preset_latency(cx: &mut gpui_kit::TestAppContext) {
    let (window, content) = mount_app(cx, None, None);
    cx.update_window(window.into(), |_, window, cx| {
        let mut samples = Vec::new();
        for _ in 0..9 {
            show_empty_workspace(&content, cx);
            window.render_frame(cx);
            let position = window.find("empty-preset-0").bounds().center();
            // `window.click` renders a frame before dispatching; the latency
            // must start at the press, so the events go straight to the app.
            let start = std::time::Instant::now();
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                    button: gpui_kit::MouseButton::Left,
                    position,
                    ..Default::default()
                }),
                cx,
            );
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                    button: gpui_kit::MouseButton::Left,
                    position,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "empty_preset p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "empty preset p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}
