//! The live loading overlay around document opening.

use super::*;

/// The open action starts the loading overlay before the background load and
/// clears it after the document installs.
#[gpui_kit::test]
fn opening_a_project_shows_and_clears_the_loading_overlay(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let path = directory.path().join("second.ptz");
    std::fs::write(&path, super::commands::multi_layer_archive_with(false))
        .expect("second fixture is written");

    cx.update_window(window.into(), |_, _window, cx| {
        content.update(cx, |app, cx| {
            app.open_path_async(path.clone(), cx);
            assert_eq!(
                app.state_for_test().read(cx).loading,
                Some(crate::state::LoadingState::Indeterminate),
                "the overlay starts before the background load"
            );
        });
    })
    .expect("test window remains open");

    cx.run_until_parked();

    cx.update_window(window.into(), |_, _window, cx| {
        content.update(cx, |app, cx| {
            assert_eq!(
                app.open_tab_ids_for_test(),
                vec!["bounded-raster-doc".to_string(), "multi-doc".to_string()],
                "the background load installs the document"
            );
            assert_eq!(
                app.state_for_test().read(cx).loading,
                None,
                "the overlay clears after the install"
            );
        });
    })
    .expect("test window remains open");
}

/// Progress clamps to the track, non-finite updates are ignored, and a second
/// open cannot replace the overlay that is already live.
#[gpui_kit::test]
fn loading_progress_is_clamped_and_a_second_open_is_refused(cx: &mut gpui_kit::TestAppContext) {
    use crate::state::LoadingState;

    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, _window, cx| {
        content.update(cx, |app, cx| {
            app.begin_loading(cx);
            app.set_loading_progress(0.42, cx);
            assert_eq!(
                app.state_for_test().read(cx).loading,
                Some(LoadingState::Progress(0.42))
            );
            app.set_loading_progress(1.7, cx);
            assert_eq!(
                app.state_for_test().read(cx).loading,
                Some(LoadingState::Progress(1.0)),
                "progress clamps to the track"
            );
            app.set_loading_progress(f32::NAN, cx);
            assert_eq!(
                app.state_for_test().read(cx).loading,
                Some(LoadingState::Progress(1.0)),
                "a non-finite update is ignored"
            );

            app.open_path_async(directory.path().join("missing.ptz"), cx);
            assert_eq!(
                app.state_for_test().read(cx).loading,
                Some(LoadingState::Progress(1.0)),
                "a second open does not replace the live overlay"
            );
            assert!(
                app.state_for_test()
                    .read(cx)
                    .status_error
                    .as_deref()
                    .is_some_and(|error| error.contains("already loading")),
                "the refused open explains itself"
            );

            app.end_loading(cx);
            assert_eq!(app.state_for_test().read(cx).loading, None);
        });
    })
    .expect("test window remains open");
}
