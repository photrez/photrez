use super::*;

#[gpui_kit::test]
fn ctrl_n_opens_the_new_document_dialog(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);

    cx.update_window(window.into(), |_, window, cx| {
        window.press("ctrl-n", cx);
        window.render_frame(cx);

        content.update(cx, |app, cx| {
            assert_eq!(
                app.state_for_test().read(cx).active_dialog,
                Some(crate::state::DialogId::NewDocument),
                "ctrl-n must open the dialog the File menu advertises"
            );
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn menu_shortcut_bindings_drive_their_commands(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        crate::app::tests::input::tab_to_canvas(window, cx);
        let (rows, selected, snap) = content.update(cx, |app, cx| {
            let state = app.state_for_test().read(cx);
            (
                state.layers.len(),
                state.selected_layers.len(),
                state.snap_enabled,
            )
        });

        window.press("ctrl-shift-n", cx);
        window.render_frame(cx);
        let after_new = content.update(cx, |app, cx| app.state_for_test().read(cx).layers.len());
        assert_eq!(after_new, rows + 1, "Ctrl+Shift+N adds a layer");

        window.press("ctrl-alt-a", cx);
        window.render_frame(cx);
        let after_select = content.update(cx, |app, cx| {
            app.state_for_test().read(cx).selected_layers.len()
        });
        assert_eq!(after_select, after_new, "Ctrl+Alt+A selects every row");

        window.press("ctrl-shift-;", cx);
        window.render_frame(cx);
        let after_snap = content.update(cx, |app, cx| app.state_for_test().read(cx).snap_enabled);
        assert_eq!(after_snap, !snap, "Shift+Ctrl+; toggles snapping");

        window.press("ctrl-shift-alt-e", cx);
        window.render_frame(cx);
        let after_stamp = content.update(cx, |app, cx| app.state_for_test().read(cx).layers.len());
        assert_eq!(
            after_stamp,
            after_new + 1,
            "Ctrl+Shift+Alt+E stamps visible"
        );
        assert!(selected >= 1, "the fixture starts with a selected row");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn ctrl_shift_p_toggles_the_side_panels(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);

    cx.update_window(window.into(), |_, window, cx| {
        assert!(
            content.read(cx).state_for_test().read(cx).dock_open,
            "the dock starts open"
        );
        window.press("ctrl-shift-p", cx);
        window.render_frame(cx);
        assert!(
            !content.read(cx).state_for_test().read(cx).dock_open,
            "Ctrl+Shift+P hides the side panels"
        );
        window.press("ctrl-shift-p", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).dock_open,
            "Ctrl+Shift+P shows them again"
        );

        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.open_menu = Some(crate::state::MenuId::View);
            });
        });
        window.render_frame(cx);
        window.click("menu-view-side-panels", cx);
        window.render_frame(cx);
        assert!(
            !content.read(cx).state_for_test().read(cx).dock_open,
            "the View menu row toggles the panels"
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).open_menu,
            None,
            "the command closes the menu"
        );
    })
    .expect("test window remains open");
}

/// The two document commands the tab strip carries: `+` opens the New Document
/// dialog and Export opens the Export dialog, both the same routes as the File
/// menu rows.
#[gpui_kit::test]
fn the_tab_strip_opens_new_document_and_export(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("export-button", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            Some(crate::state::DialogId::Export),
            "the strip's Export button opens the Export dialog"
        );

        content.update(cx, |app, cx| {
            app.state_for_test()
                .update(cx, |state, _| state.active_dialog = None);
        });
        window.click("new-document", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            Some(crate::state::DialogId::NewDocument),
            "the strip's plus button opens the New Document dialog"
        );
    })
    .expect("test window remains open");
}

/// The status bar's History trigger shows the dock's History panel, and opens
/// the dock when the user had hidden it.
#[gpui_kit::test]
fn the_status_history_trigger_opens_the_history_panel(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);

    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.layer_tab = crate::state::LayerTab::Layers;
                state.dock_open = false;
            });
        });
        window.render_frame(cx);

        window.click("status-history", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(
            state.layer_tab,
            crate::state::LayerTab::History,
            "the trigger switches the dock to History"
        );
        assert!(state.dock_open, "and opens a dock the user had hidden");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn ctrl_s_rewrites_the_source_archive_and_clears_dirty(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let (window, content, source) = mount_fixture_app(cx, &directory);

    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, _| {
            assert_eq!(
                app.active_document_id_for_test().as_deref(),
                Some("bounded-raster-doc")
            );
            assert_eq!(
                app.active_document_save_state_for_test(),
                Some((0, 0, false))
            );
            assert_eq!(
                app.mark_active_document_dirty_for_test()
                    .expect("active session accepts an edit"),
                1
            );
            assert_eq!(
                app.active_document_save_state_for_test(),
                Some((1, 0, true))
            );
        });

        // A rewrite is observable only when the old archive is gone.
        std::fs::remove_file(&source).expect("fixture is removed before save");
        window.press("ctrl-s", cx);
        window.render_frame(cx);

        content.update(cx, |app, cx| {
            assert_eq!(
                app.active_document_save_state_for_test(),
                Some((1, 1, false))
            );
            assert_eq!(
                app.active_document_id_for_test().as_deref(),
                Some("bounded-raster-doc")
            );
            assert!(
                app.state_for_test().read(cx).status_error.is_none(),
                "a successful save must clear the error line"
            );
        });
    })
    .expect("test window remains open");

    assert!(source.is_file(), "save must recreate the source archive");
    let file = std::fs::File::open(&source).expect("saved archive opens");
    let mut archive = zip::ZipArchive::new(file).expect("saved archive is complete");
    assert!(archive.by_name("document.json").is_ok());
    let mut layer = archive
        .by_name("layers/raster-1.png")
        .expect("the raster layer is written");
    let mut png = Vec::new();
    std::io::Read::read_to_end(&mut layer, &mut png).expect("layer entry reads");
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
}

/// A failed save must leave the session open and dirty, surface the typed
/// error in the status bar, and recover on the next successful save.
#[gpui_kit::test]
fn ctrl_s_failure_keeps_dirty_and_surfaces_the_error(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let (window, content, source) = mount_fixture_app(cx, &directory);

    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, _| {
            app.mark_active_document_dirty_for_test()
                .expect("active session accepts an edit");
        });
        std::fs::remove_file(&source).expect("fixture is removed before save");
        std::fs::create_dir(&source).expect("a directory occupies the save target");

        window.press("ctrl-s", cx);
        window.render_frame(cx);
        assert!(
            window.find("status-error").visible(),
            "the failed save must mount the status-bar error cell"
        );

        content.update(cx, |app, cx| {
            assert_eq!(
                app.active_document_save_state_for_test(),
                Some((1, 0, true)),
                "a failed save must not mark the document clean"
            );
            assert_eq!(
                app.active_document_id_for_test().as_deref(),
                Some("bounded-raster-doc"),
                "a failed save must keep the document open"
            );
            let state = app.state_for_test().read(cx);
            let message = state
                .status_error
                .as_deref()
                .expect("the service error is shown in the status bar");
            assert!(message.contains("E_VALIDATION"), "{message}");
            assert!(
                message.contains("save target must be a regular file"),
                "{message}"
            );
        });

        // Retry once the obstruction is gone: the session is not poisoned.
        std::fs::remove_dir(&source).expect("remove the obstructing directory");
        window.press("ctrl-s", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("status-error").is_none(),
            "a successful retry must unmount the status-bar error cell"
        );

        content.update(cx, |app, cx| {
            assert_eq!(
                app.active_document_save_state_for_test(),
                Some((1, 1, false))
            );
            assert!(
                app.state_for_test().read(cx).status_error.is_none(),
                "a successful retry must clear the error line"
            );
        });
    })
    .expect("test window remains open");

    assert!(source.is_file(), "the retry must write the archive");
    let file = std::fs::File::open(&source).expect("saved archive opens");
    zip::ZipArchive::new(file).expect("saved archive is complete");
}

/// Opening another document must not discard unsaved work: the active
/// dirty session survives the refused open.
#[gpui_kit::test]
fn open_refuses_to_replace_a_dirty_document(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, source) = mount_fixture_app(cx, &directory);
    let other = directory.path().join("other.ptz");
    std::fs::write(&other, BOUNDED_RASTER).expect("the replacement fixture is copied");

    cx.update_window(window.into(), |_, _window, cx| {
        content.update(cx, |app, cx| {
            app.mark_active_document_dirty_for_test()
                .expect("active session accepts an edit");
            let file_io = app.file_io.clone().expect("the file boundary is open");
            let error = app
                .open_startup_document_for_test(&other, file_io.as_ref(), cx)
                .expect_err("a dirty document is not replaced");
            assert!(error.contains("unsaved changes"), "{error}");
            assert_eq!(
                app.active_document_id_for_test().as_deref(),
                Some("bounded-raster-doc")
            );
            assert_eq!(
                app.active_document_save_state_for_test(),
                Some((1, 0, true))
            );
            assert!(app.state_for_test().read(cx).status_error.is_none());
        });
    })
    .expect("test window remains open");

    assert!(source.is_file());
}

#[derive(Default)]
struct RecordingPrintBackend {
    calls: std::sync::Mutex<Vec<RecordedPrint>>,
    fail: std::sync::atomic::AtomicBool,
}

struct RecordedPrint {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    document_name: String,
    paper_mm: (f64, f64),
    output_path: Option<String>,
}

impl crate::services::print::PrintDispatch for RecordingPrintBackend {
    fn print(
        &self,
        request: &crate::services::print::PrintRequest<'_>,
        cancellation: &crate::services::print::PrintCancellation,
    ) -> Result<(), crate::services::print::PrintError> {
        assert!(!cancellation.is_cancelled());
        self.calls.lock().unwrap().push(RecordedPrint {
            width: request.width,
            height: request.height,
            pixels: request.pixels.to_vec(),
            document_name: request.document_name.to_owned(),
            paper_mm: request.settings.oriented_dimensions(),
            output_path: request.output_path.map(str::to_owned),
        });
        if self.fail.load(Ordering::Relaxed) {
            Err(crate::services::print::PrintError::Printer(
                "injected printer failure".into(),
            ))
        } else {
            Ok(())
        }
    }
}

/// Ctrl+P must hand the active document's validated pixels to the print
/// boundary with the default paper and no output file.
///
/// RED proof: remove the `ctrl-p` binding or the print handler and the
/// backend records no call.
#[gpui_kit::test]
fn ctrl_p_sends_the_active_document_to_the_print_backend(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let backend = Arc::new(RecordingPrintBackend::default());

    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, _| {
            app.set_print_backend_for_test(Arc::clone(&backend) as Arc<dyn PrintDispatch>);
        });
        window.press("ctrl-p", cx);
        window.render_frame(cx);
        window.click("print-confirm", cx);
        window.render_frame(cx);

        content.update(cx, |app, cx| {
            assert!(
                app.state_for_test().read(cx).status_error.is_none(),
                "a successful print must not leave a status error"
            );
        });
    })
    .expect("test window remains open");

    let calls = backend.calls.lock().unwrap();
    assert_eq!(calls.len(), 1, "Ctrl+P must reach the print boundary once");
    let call = &calls[0];
    assert_eq!((call.width, call.height), (2, 1));
    assert_eq!(call.pixels, EXPECTED_CANONICAL_RGBA);
    assert_eq!(call.document_name, "Bounded raster document");
    assert_eq!(call.paper_mm, (210.0, 297.0));
    assert_eq!(call.output_path, None);
}

/// A two-layer 2x1 project: a blue 1x1 top raster over a red 2x1 bottom.
/// Saving or printing it must use the composited bytes, not one layer.
fn multi_layer_archive() -> Vec<u8> {
    multi_layer_archive_with(false)
}

/// Same two-layer project with the top layer optionally locked, for the
/// selected-layer paint tests.
pub(super) fn multi_layer_archive_with(top_locked: bool) -> Vec<u8> {
    multi_layer_archive_layers(top_locked, false, false)
}

/// Same two-layer project with the top layer carrying a basic adjustment, for
/// the layer row's adjustments indicator.
pub(super) fn multi_layer_archive_with_adjusted_top() -> Vec<u8> {
    multi_layer_archive_layers(false, true, false)
}

/// Same two-layer project with the bottom layer marked as the document's
/// Background, for the row lock's background rule.
pub(super) fn multi_layer_archive_with_background() -> Vec<u8> {
    multi_layer_archive_layers(false, false, true)
}

fn multi_layer_archive_layers(
    top_locked: bool,
    top_adjusted: bool,
    bottom_background: bool,
) -> Vec<u8> {
    use std::io::Write as _;

    let png = |width: u32, height: u32, pixels: &[u8]| {
        photrez_core::pixel::export::encode_image(width, height, pixels, "png", 100)
            .expect("test PNG encodes")
    };
    let layer = |id: &str, width: u32, height: u32, locked: bool| {
        let mut layer = serde_json::json!({
            "id": id,
            "name": id,
            "type": "raster",
            "visible": true,
            "opacity": 1.0,
            "locked": locked,
            "blendMode": "normal",
            "transform": {
                "x": 0.0,
                "y": 0.0,
                "scaleX": 1.0,
                "scaleY": 1.0,
                "rotation": 0.0,
                "flipH": false,
                "flipV": false
            },
            "width": width,
            "height": height
        });
        if id == "top" && top_adjusted {
            layer["hasAdjustments"] = serde_json::json!(true);
        }
        if id == "bottom" && bottom_background {
            layer["isBackground"] = serde_json::json!(true);
        }
        layer
    };
    let metadata = serde_json::to_vec(&serde_json::json!({
        "format": "photrez-ptz",
        "version": 3,
        "id": "multi-doc",
        "name": "Multi layer",
        "width": 2,
        "height": 1,
        "layers": [
            layer("top", 1, 1, top_locked),
            layer("bottom", 2, 1, false)
        ]
    }))
    .expect("multi-layer metadata serializes");
    let top = png(1, 1, &[0, 0, 255, 255]);
    let bottom = png(2, 1, &[255, 0, 0, 255, 255, 0, 0, 255]);

    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in [
        ("document.json", metadata.as_slice()),
        ("layers/top.png", top.as_slice()),
        ("layers/bottom.png", bottom.as_slice()),
    ] {
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .expect("ZIP entry starts");
        writer.write_all(bytes).expect("ZIP entry is written");
    }
    writer.finish().expect("ZIP archive finishes").into_inner()
}

/// A multi-layer project must open and print its composited bytes, not a
/// single layer's pixels.
#[gpui_kit::test]
fn ctrl_p_prints_the_composited_multi_layer_document(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let source = directory.path().join("multi.ptz");
    std::fs::write(&source, multi_layer_archive()).expect("multi-layer fixture is written");
    let file_io = Arc::new(
        LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
            .expect("local file boundary opens"),
    );
    file_io.approve_path(&source).expect("fixture is approved");
    let (window, content) = mount_app(cx, Some(source), Some(file_io));
    let backend = Arc::new(RecordingPrintBackend::default());

    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, _| {
            let active = app
                .active_document
                .as_ref()
                .expect("the multi-layer document is active");
            assert_eq!(active.doc_id, "multi-doc");
            assert_eq!(active.canvas.tile_count(), 1);
            app.set_print_backend_for_test(Arc::clone(&backend) as Arc<dyn PrintDispatch>);
        });
        window.press("ctrl-p", cx);
        window.render_frame(cx);
        window.click("print-confirm", cx);
        window.render_frame(cx);
    })
    .expect("test window remains open");

    let calls = backend.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!((calls[0].width, calls[0].height), (2, 1));
    assert_eq!(
        calls[0].pixels,
        [0, 0, 255, 255, 255, 0, 0, 255],
        "print must receive the composited layer stack"
    );
}

/// Ctrl+S must preserve every layer of a multi-layer project, not just the
/// first raster.
#[gpui_kit::test]
fn ctrl_s_saves_all_layers_of_a_multi_layer_document(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let source = directory.path().join("multi.ptz");
    std::fs::write(&source, multi_layer_archive()).expect("multi-layer fixture is written");
    let file_io = Arc::new(
        LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
            .expect("local file boundary opens"),
    );
    file_io.approve_path(&source).expect("fixture is approved");
    let (window, content) = mount_app(cx, Some(source.clone()), Some(file_io));

    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, _| {
            assert_eq!(
                app.active_document_id_for_test().as_deref(),
                Some("multi-doc")
            );
            app.mark_active_document_dirty_for_test()
                .expect("active session accepts an edit");
        });
        std::fs::remove_file(&source).expect("fixture is removed before save");
        window.press("ctrl-s", cx);
        window.render_frame(cx);
        content.update(cx, |app, _| {
            assert_eq!(
                app.active_document_save_state_for_test(),
                Some((1, 1, false))
            );
        });
    })
    .expect("test window remains open");

    let file = std::fs::File::open(&source).expect("save recreated the archive");
    let mut archive = zip::ZipArchive::new(file).expect("saved archive is complete");
    let mut entries = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).expect("archive entry opens");
        entries.push(
            std::str::from_utf8(entry.name_raw())
                .expect("entry name is UTF-8")
                .to_owned(),
        );
    }
    entries.sort();
    assert_eq!(
        entries,
        [
            "document.json".to_owned(),
            "layers/bottom.png".to_owned(),
            "layers/top.png".to_owned(),
        ]
    );
}

/// The Open dialog imports a raster, and Ctrl+S on the imported document
/// redirects to Save As because it has no project path yet.
#[gpui_kit::test]
fn ctrl_o_imports_an_image_and_ctrl_s_prompts_for_a_project_path(
    cx: &mut gpui_kit::TestAppContext,
) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let image_path = directory.path().join("photo.png");
    let pixels = [
        255u8, 0, 0, 255, 0, 0, 255, 255, 0, 255, 0, 255, 255, 255, 0, 255,
    ];
    let png = photrez_core::pixel::export::encode_image(2, 2, &pixels, "png", 100)
        .expect("test image encodes");
    std::fs::write(&image_path, png).expect("test image is written");
    let file_io = Arc::new(
        LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
            .expect("local file boundary opens"),
    );
    let (window, content) = mount_app(cx, None, Some(file_io));

    cx.update_window(window.into(), |_, window, cx| {
        window.press("ctrl-o", cx);
    })
    .expect("test window remains open");
    assert!(cx.did_prompt_for_paths(), "Ctrl+O must open the picker");
    cx.simulate_path_prompt_response(|_| Some(vec![image_path.clone()]));
    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, _| {
            let id = app
                .active_document_id_for_test()
                .expect("the imported document is active");
            assert!(id.starts_with("import-"), "{id}");
            assert_eq!(
                app.active_document_save_state_for_test(),
                Some((0, 0, false)),
                "an imported image starts clean"
            );
        });
        window.press("ctrl-s", cx);
    })
    .expect("test window remains open");
    assert!(
        cx.did_prompt_for_new_path(),
        "Ctrl+S on an imported image must ask for a project path"
    );

    let saved_path = directory.path().join("saved.ptz");
    cx.simulate_new_path_selection({
        let saved_path = saved_path.clone();
        move |_| Some(saved_path)
    });
    cx.run_until_parked();

    assert!(saved_path.is_file(), "Save As must write the project");
    let file = std::fs::File::open(&saved_path).expect("saved project opens");
    let mut archive = zip::ZipArchive::new(file).expect("saved project is an archive");
    assert!(archive.by_name("document.json").is_ok());
    assert!(archive.by_name("layers/background.png").is_ok());
}

/// The Open dialog routes a `.ptz` choice through the project loader.
#[gpui_kit::test]
fn ctrl_o_opens_a_project_through_the_dialog(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let project_path = directory.path().join("project.ptz");
    std::fs::write(&project_path, BOUNDED_RASTER).expect("project fixture is written");
    let file_io = Arc::new(
        LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
            .expect("local file boundary opens"),
    );
    let (window, content) = mount_app(cx, None, Some(file_io));

    cx.update_window(window.into(), |_, window, cx| {
        window.press("ctrl-o", cx);
    })
    .expect("test window remains open");
    cx.simulate_path_prompt_response(|_| Some(vec![project_path]));
    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        content.update(cx, |app, _| {
            assert_eq!(
                app.active_document_id_for_test().as_deref(),
                Some("bounded-raster-doc")
            );
            assert_eq!(
                app.active_document_save_state_for_test(),
                Some((0, 0, false))
            );
        });
    })
    .expect("test window remains open");
}

/// Ctrl+Shift+S writes the active document to the path chosen in the
/// platform dialog.
#[gpui_kit::test]
fn ctrl_shift_s_saves_to_the_chosen_path(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let target = directory.path().join("copy.ptz");

    cx.update_window(window.into(), |_, window, cx| {
        window.press("ctrl-shift-s", cx);
    })
    .expect("test window remains open");
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection({
        let target = target.clone();
        move |_| Some(target)
    });
    cx.run_until_parked();

    assert!(target.is_file(), "Save As must write the chosen path");
    cx.update_window(window.into(), |_, _, cx| {
        content.update(cx, |app, _| {
            assert_eq!(
                app.active_document_save_state_for_test(),
                Some((0, 0, false))
            );
        });
    })
    .expect("test window remains open");
    let file = std::fs::File::open(&target).expect("saved project opens");
    let mut archive = zip::ZipArchive::new(file).expect("saved project is an archive");
    assert!(archive.by_name("document.json").is_ok());
    assert!(archive.by_name("layers/raster-1.png").is_ok());
}

/// Ctrl+Alt+E writes the composite in the format named by the chosen path.
#[gpui_kit::test]
fn ctrl_alt_e_exports_the_chosen_format(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let (window, content, source) = mount_fixture_app(cx, &directory);
    let png_path = directory.path().join("export.png");

    cx.update_window(window.into(), |_, window, cx| {
        window.press("ctrl-alt-e", cx);
        window.render_frame(cx);
        window.click("export-confirm", cx);
        window.render_frame(cx);
    })
    .expect("test window remains open");
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection({
        let png_path = png_path.clone();
        move |_| Some(png_path)
    });
    cx.run_until_parked();

    let png = std::fs::read(&png_path).expect("PNG export exists");
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    let dimensions = photrez_core::pixel::export::image_dimensions(&png, image::ImageFormat::Png)
        .expect("exported PNG probes");
    assert_eq!((dimensions.width(), dimensions.height()), (2, 1));

    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, _| {
            assert_eq!(
                app.active_document_save_state_for_test(),
                Some((0, 0, false)),
                "export must not change the save state"
            );
        });
        window.press("ctrl-alt-e", cx);
        window.render_frame(cx);
        window.click("export-confirm", cx);
        window.render_frame(cx);
    })
    .expect("test window remains open");
    let jpeg_path = directory.path().join("export.jpg");
    cx.simulate_new_path_selection(move |_| Some(jpeg_path.clone()));
    cx.run_until_parked();
    let jpeg = std::fs::read(directory.path().join("export.jpg")).expect("JPEG export exists");
    assert!(jpeg.starts_with(&[0xff, 0xd8, 0xff]));
    assert!(source.is_file(), "the source project is untouched");
}

/// Dirty sessions autosave into the app-cache store without clearing dirty.
#[gpui_kit::test]
fn autosave_writes_dirty_sessions_to_the_cache(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, source) = mount_fixture_app(cx, &directory);
    let cache_root = directory.path().join("cache");
    std::fs::create_dir_all(&cache_root).expect("cache root is created");

    cx.update_window(window.into(), |_, _, cx| {
        content.update(cx, |app, _| {
            assert_eq!(
                app.autosave_dirty_sessions(&cache_root)
                    .expect("a clean session writes nothing"),
                0
            );
            app.mark_active_document_dirty_for_test()
                .expect("active session accepts an edit");
            assert_eq!(
                app.autosave_dirty_sessions(&cache_root)
                    .expect("a dirty session autosaves"),
                1
            );
            assert_eq!(
                app.active_document_save_state_for_test(),
                Some((1, 0, true)),
                "autosave must not mark the document clean"
            );

            let store = crate::services::autosave::AutosaveStore::new(&cache_root);
            let entries = store.list().expect("the manifest reads");
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].doc_id(), "bounded-raster-doc");
            let path = store
                .recovery_path(&entries[0])
                .expect("the recovery path is inside the cache");
            assert!(path.is_file(), "the autosave archive exists");
            let file = std::fs::File::open(&path).expect("autosave archive opens");
            let mut archive = zip::ZipArchive::new(file).expect("autosave is an archive");
            assert!(archive.by_name("document.json").is_ok());
            assert!(archive.by_name("layers/raster-1.png").is_ok());
        });
    })
    .expect("test window remains open");

    assert!(
        source.is_file(),
        "autosave must not touch the source project"
    );
}

/// A failed print must keep the document open, report the typed error, and
/// clear it on the next successful attempt.
#[gpui_kit::test]
fn ctrl_p_failure_reports_and_recovers(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let backend = Arc::new(RecordingPrintBackend::default());
    backend
        .fail
        .store(true, std::sync::atomic::Ordering::Relaxed);

    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, _| {
            app.set_print_backend_for_test(Arc::clone(&backend) as Arc<dyn PrintDispatch>);
        });
        window.press("ctrl-p", cx);
        window.render_frame(cx);
        window.click("print-confirm", cx);
        window.render_frame(cx);
        assert!(
            window.find("status-error").visible(),
            "a failed print must mount the status-bar error cell"
        );

        content.update(cx, |app, cx| {
            assert_eq!(
                app.active_document_id_for_test().as_deref(),
                Some("bounded-raster-doc"),
                "a failed print must keep the document open"
            );
            assert_eq!(
                app.active_document_save_state_for_test(),
                Some((0, 0, false)),
                "printing must not change the save state"
            );
            let message = app
                .state_for_test()
                .read(cx)
                .status_error
                .as_deref()
                .expect("the print failure is shown")
                .to_owned();
            assert!(message.contains("Print failed (E_PRINTER)"), "{message}");
        });

        backend
            .fail
            .store(false, std::sync::atomic::Ordering::Relaxed);
        window.press("ctrl-p", cx);
        window.render_frame(cx);
        window.click("print-confirm", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("status-error").is_none(),
            "a successful retry must unmount the error cell"
        );
        content.update(cx, |app, cx| {
            assert!(app.state_for_test().read(cx).status_error.is_none());
        });
    })
    .expect("test window remains open");

    let calls = backend.calls.lock().unwrap();
    assert_eq!(calls.len(), 2, "the retry must reach the boundary again");
}

/// Printing with no document must report a typed conflict instead of
/// silently doing nothing.
#[gpui_kit::test]
fn ctrl_p_without_a_document_reports_a_conflict(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let backend = Arc::new(RecordingPrintBackend::default());
    let (handle, content) = mount_app(cx, None, None);
    cx.update_window(handle.into(), |_, window, cx| {
        content.update(cx, |app, _| {
            app.set_print_backend_for_test(Arc::clone(&backend) as Arc<dyn PrintDispatch>);
        });
        window.press("ctrl-p", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            let message = app
                .state_for_test()
                .read(cx)
                .status_error
                .as_deref()
                .expect("the conflict is shown")
                .to_owned();
            assert!(message.contains("Print failed (E_CONFLICT)"), "{message}");
        });
    })
    .expect("test window remains open");
    assert!(backend.calls.lock().unwrap().is_empty());
}
