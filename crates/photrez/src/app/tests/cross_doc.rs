//! Cross-document layer drag and file-drop routing.
//!
//! The GPUI drag transport (pointer-down on a row, drop hit-test) is not
//! drivable from the mounted harness; these tests pin the production handler
//! that every drop target calls with the payload GPUI would deliver.

use super::*;
use crate::app::cross_doc::{InsertPosition, LayerDragPayload, LayerDropTarget};
use gpui_kit::test::TestWindowExt;
use photrez_core::document::content::{PtzDocumentContent, PtzLayer};

const FIXTURE_DOC: &str = "bounded-raster-doc";
const FIXTURE_PNG: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/bounded-raster.layer.png"
);

fn content(app: &Entity<App>, cx: &gpui_kit::App, doc_id: &str) -> PtzDocumentContent {
    app.read(cx)
        .sessions
        .get(doc_id)
        .expect("the document is open")
        .loaded_document()
        .content_snapshot()
        .expect("the document has content")
        .clone()
}

fn layer_count(app: &Entity<App>, cx: &gpui_kit::App, doc_id: &str) -> usize {
    content(app, cx, doc_id).layers.len()
}

fn copied_layer<'a>(before: &[PtzLayer], after: &'a [PtzLayer]) -> &'a PtzLayer {
    after
        .iter()
        .find(|layer| !before.iter().any(|known| known.id == layer.id))
        .expect("the drop creates one layer")
}

#[gpui_kit::test]
fn cross_document_layer_drag_copies_pixels_and_state(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, app, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let source_ids = app.read(cx).active_core_layer_ids_for_test();
        let source_id = source_ids.first().cloned().expect("fixture layer");
        let source_pixels = app
            .read(cx)
            .core_layer_pixels_for_test(&source_id)
            .expect("fixture layer pixels");
        let source_before = content(&app, cx, FIXTURE_DOC).layers;

        app.update(cx, |app, cx| {
            app.create_document(8, 8, cx)
                .expect("blank target document");
        });
        window.render_frame(cx);
        let target_doc = app
            .read(cx)
            .open_tab_ids_for_test()
            .last()
            .cloned()
            .expect("target tab");
        let target_before = content(&app, cx, &target_doc).layers;
        app.update(cx, |app, cx| {
            app.layer_drag_impl(
                &LayerDragPayload {
                    source_doc_id: FIXTURE_DOC.to_string(),
                    core_id: Some(source_id.clone()),
                    row_id: 0,
                },
                LayerDropTarget::Tab {
                    doc_id: target_doc.clone(),
                },
                false,
                cx,
            )
            .expect("cross-document copy");
        });
        window.render_frame(cx);

        let target_after = content(&app, cx, &target_doc).layers;
        assert_eq!(target_after.len(), target_before.len() + 1);
        let copied = copied_layer(&target_before, &target_after);
        assert_eq!(copied.width, source_before[0].width);
        assert_eq!(copied.height, source_before[0].height);
        assert_eq!(copied.opacity, source_before[0].opacity);
        assert_eq!(copied.blend_mode, source_before[0].blend_mode);
        assert_eq!(copied.visible, source_before[0].visible);
        // A tab drop has no canvas cursor, so it centers in the target.
        assert_eq!(
            copied.transform.x,
            ((8.0_f64 - copied.width) / 2.0).max(0.0)
        );
        assert_eq!(
            copied.transform.y,
            ((8.0_f64 - copied.height) / 2.0).max(0.0)
        );
        let copied_pixels = app
            .read(cx)
            .core_layer_pixels_for_test(&copied.id)
            .expect("the copy owns pixels");
        assert_eq!(
            (copied_pixels.0, copied_pixels.1),
            (source_pixels.0, source_pixels.1)
        );
        assert_eq!(copied_pixels.2, source_pixels.2);

        // The copy leaves the source document untouched.
        assert_eq!(content(&app, cx, FIXTURE_DOC).layers, source_before);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn alt_layer_drag_moves_and_guards_the_last_source_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, app, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let fixture_layer = app
            .read(cx)
            .active_core_layer_ids_for_test()
            .first()
            .cloned()
            .expect("fixture layer");
        app.update(cx, |app, cx| {
            app.create_document(8, 8, cx)
                .expect("blank target document");
        });
        window.render_frame(cx);
        let target_doc = app
            .read(cx)
            .open_tab_ids_for_test()
            .last()
            .cloned()
            .expect("target tab");

        // The fixture owns a single layer; ALT must refuse to empty it.
        let refusal = app.update(cx, |app, cx| {
            app.layer_drag_impl(
                &LayerDragPayload {
                    source_doc_id: FIXTURE_DOC.to_string(),
                    core_id: Some(fixture_layer),
                    row_id: 0,
                },
                LayerDropTarget::Tab {
                    doc_id: target_doc.clone(),
                },
                true,
                cx,
            )
        });
        assert_eq!(
            refusal,
            Err("Cannot move the last layer from a document. Drop cancelled.".to_string())
        );
        assert_eq!(layer_count(&app, cx, FIXTURE_DOC), 1);

        // Move the fixture's duplicate into the target document.
        app.update(cx, |app, cx| {
            app.activate_document(FIXTURE_DOC, cx)
                .expect("the source document reopens");
        });
        window.render_frame(cx);
        let moved_id = app.update(cx, |app, cx| {
            app.duplicate_selected_layer_id(cx)
                .expect("the fixture layer can be duplicated")
        });
        window.render_frame(cx);
        assert_eq!(layer_count(&app, cx, FIXTURE_DOC), 2);
        app.update(cx, |app, cx| {
            app.layer_drag_impl(
                &LayerDragPayload {
                    source_doc_id: FIXTURE_DOC.to_string(),
                    core_id: Some(moved_id),
                    row_id: 0,
                },
                LayerDropTarget::Tab {
                    doc_id: target_doc.clone(),
                },
                true,
                cx,
            )
            .expect("ALT moves the layer");
        });
        window.render_frame(cx);
        assert_eq!(layer_count(&app, cx, FIXTURE_DOC), 1);
        assert_eq!(layer_count(&app, cx, &target_doc), 2);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn layer_drag_on_the_empty_workspace_seeds_a_document(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, app, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let source_layer = content(&app, cx, FIXTURE_DOC).layers[0].clone();
        let tabs_before = app.read(cx).open_tab_ids_for_test();
        app.update(cx, |app, cx| {
            app.layer_drag_impl(
                &LayerDragPayload {
                    source_doc_id: FIXTURE_DOC.to_string(),
                    core_id: Some(source_layer.id.clone()),
                    row_id: 0,
                },
                LayerDropTarget::NewDocument,
                false,
                cx,
            )
            .expect("empty-workspace drop seeds a document");
        });
        window.render_frame(cx);
        let tabs = app.read(cx).open_tab_ids_for_test();
        assert_eq!(tabs.len(), tabs_before.len() + 1);
        let new_doc = tabs.last().cloned().expect("new document tab");
        let seeded = content(&app, cx, &new_doc);
        assert_eq!(
            (seeded.width, seeded.height),
            (source_layer.width, source_layer.height)
        );
        // The seeded background plus the copied layer.
        assert_eq!(seeded.layers.len(), 2);
        let copied = seeded
            .layers
            .iter()
            .find(|layer| layer.id != seeded.layers[0].id || layer.name == source_layer.name)
            .expect("copied layer");
        assert_eq!(copied.transform.x, 0.0);
        assert_eq!(copied.transform.y, 0.0);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn file_drop_adds_centered_layers_with_the_cascade_offset(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, app, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let dropped = directory.path().join("dropped.png");
        std::fs::copy(FIXTURE_PNG, &dropped).expect("fixture image is copied");
        let before = content(&app, cx, FIXTURE_DOC).layers;
        let added = app.update(cx, |app, cx| {
            app.drop_files_as_layers(
                &[dropped.clone(), dropped],
                FIXTURE_DOC,
                (100.0, 120.0),
                true,
                cx,
            )
        });
        assert_eq!(added, Ok(2));
        window.render_frame(cx);
        let after = content(&app, cx, FIXTURE_DOC).layers;
        assert_eq!(after.len(), before.len() + 2);
        let mut dropped: Vec<&PtzLayer> = after
            .iter()
            .filter(|layer| !before.iter().any(|known| known.id == layer.id))
            .collect();
        dropped.sort_by(|a, b| a.transform.x.total_cmp(&b.transform.x));
        let first = dropped[0];
        let second = dropped[1];
        assert_eq!(first.transform.x, 100.0 - first.width / 2.0);
        assert_eq!(first.transform.y, 120.0 - first.height / 2.0);
        assert_eq!(second.transform.x, first.transform.x + 24.0);
        assert_eq!(second.transform.y, first.transform.y + 24.0);
        assert_eq!(first.name, "dropped.png");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn mixed_file_drop_uses_the_original_target_camera_for_raster_placement(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, app, _source) = mount_fixture_app(cx, &directory);
    let project = directory.path().join("dropped.ptz");
    std::fs::write(&project, super::commands::multi_layer_archive_with(false))
        .expect("dropped project is written");
    let image = directory.path().join("dropped.png");
    std::fs::copy(FIXTURE_PNG, &image).expect("dropped image is copied");

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let position = window.find("canvas-hitbox").bounds().center();
        let expected_center = app.update(cx, |app, cx| {
            let file_io = app.file_io.clone().expect("file boundary is open");
            file_io
                .approve_path(&project)
                .expect("dropped project is approved");
            file_io
                .approve_path(&image)
                .expect("dropped image is approved");

            app.set_camera(
                crate::display::canvas_bridge::Camera::new(11.0, 22.0, 2.0)
                    .expect("target camera is valid"),
                cx,
            );
            let (width, height) = app
                .active_document_size()
                .expect("the original target document is active");
            crate::app::canvas_doc_coords_free(
                position,
                &app.canvas_bounds,
                (width.max(1.0) as u32, height.max(1.0) as u32),
                app.active_camera(),
            )
            .expect("drop point maps to the target document")
        });
        let before = content(&app, cx, FIXTURE_DOC).layers;

        app.update(cx, |app, cx| {
            app.stage_dropped_paths(vec![project.clone(), image.clone()], position);
            app.submit_dropped_paths(cx);
        });
        window.render_frame(cx);

        assert_eq!(
            app.read(cx).active_tab_id_for_test().as_deref(),
            Some("multi-doc")
        );
        let after = content(&app, cx, FIXTURE_DOC).layers;
        assert_eq!(after.len(), before.len() + 1);
        let imported = after
            .iter()
            .find(|layer| layer.name == "dropped.png")
            .expect("the raster is added to the original target");
        assert_eq!(
            imported.transform.x + imported.width / 2.0,
            expected_center.0,
            "the opened project must not replace the drop target camera"
        );
        assert_eq!(
            imported.transform.y + imported.height / 2.0,
            expected_center.1,
            "the opened project must not replace the drop target camera"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn layer_drag_refuses_a_full_target_document(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, app, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let source_layer = content(&app, cx, FIXTURE_DOC).layers[0].clone();
        app.update(cx, |app, cx| {
            app.create_document(1, 1, cx)
                .expect("blank target document");
        });
        window.render_frame(cx);
        let target_doc = app
            .read(cx)
            .open_tab_ids_for_test()
            .last()
            .cloned()
            .expect("target tab");
        let mut next_id = 0;
        while layer_count(&app, cx, &target_doc) < crate::app::cross_doc::MAX_LAYERS {
            next_id += 1;
            let id = format!("fill-{next_id}");
            app.update(cx, |app, cx| {
                app.seed_layer(&target_doc, &id, 1, 1, vec![0, 0, 0, 0])
                    .expect("fill pixels");
                app.apply_doc_command(
                    &target_doc,
                    photrez_core::protocol::Command::AddLayer {
                        id,
                        name: "fill".to_string(),
                        width: 1.0,
                        height: 1.0,
                        index: 0,
                        x: None,
                        y: None,
                        layer_type: Some(photrez_core::document::content::LayerType::Raster),
                        shape_params: None,
                        text_data: None,
                    },
                    cx,
                )
                .expect("fill layer");
            });
        }
        let refusal = app.update(cx, |app, cx| {
            app.layer_drag_impl(
                &LayerDragPayload {
                    source_doc_id: FIXTURE_DOC.to_string(),
                    core_id: Some(source_layer.id),
                    row_id: 0,
                },
                LayerDropTarget::Tab {
                    doc_id: target_doc.clone(),
                },
                false,
                cx,
            )
        });
        assert_eq!(
            refusal,
            Err(format!(
                "Target document reached max {} layers",
                crate::app::cross_doc::MAX_LAYERS
            ))
        );
        assert_eq!(
            layer_count(&app, cx, &target_doc),
            crate::app::cross_doc::MAX_LAYERS
        );
    })
    .expect("test window remains open");
}

fn layer_stack_ids(app: &Entity<App>, cx: &gpui_kit::App, doc_id: &str) -> Vec<String> {
    content(app, cx, doc_id)
        .layers
        .iter()
        .map(|layer| layer.id.clone())
        .collect()
}

#[gpui_kit::test]
fn same_document_drop_above_keeps_the_source_above_the_target_row(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, app, _source) = super::canvas_input::mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let rows = super::canvas_input::layer_rows(&app, cx).0;
        assert_eq!(rows.len(), 2);
        let doc_id = app
            .read(cx)
            .active_document_id_for_test()
            .expect("fixture archive is the active document");
        app.update(cx, |app, cx| {
            app.drop_layer_on_row(
                &LayerDragPayload {
                    source_doc_id: doc_id.clone(),
                    core_id: rows[0].core_id.clone(),
                    row_id: rows[0].id,
                },
                rows[1].id,
                InsertPosition::Above,
                false,
                cx,
            );
        });
        window.render_frame(cx);
        assert_eq!(
            layer_stack_ids(&app, cx, &doc_id),
            vec!["top".to_string(), "bottom".to_string()],
            "the above drop leaves the stack unchanged"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn same_document_drop_below_inserts_the_source_below_the_target_row(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, app, _source) = super::canvas_input::mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let rows = super::canvas_input::layer_rows(&app, cx).0;
        let doc_id = app
            .read(cx)
            .active_document_id_for_test()
            .expect("fixture archive is the active document");
        app.update(cx, |app, cx| {
            app.drop_layer_on_row(
                &LayerDragPayload {
                    source_doc_id: doc_id.clone(),
                    core_id: rows[0].core_id.clone(),
                    row_id: rows[0].id,
                },
                rows[1].id,
                InsertPosition::Below,
                false,
                cx,
            );
        });
        window.render_frame(cx);
        assert_eq!(
            layer_stack_ids(&app, cx, &doc_id),
            vec!["bottom".to_string(), "top".to_string()],
            "the below drop lands the source under the target row"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn same_document_drop_above_works_when_the_source_sits_below_the_target_row(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, app, _source) = super::canvas_input::mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let rows = super::canvas_input::layer_rows(&app, cx).0;
        let doc_id = app
            .read(cx)
            .active_document_id_for_test()
            .expect("fixture archive is the active document");
        app.update(cx, |app, cx| {
            app.drop_layer_on_row(
                &LayerDragPayload {
                    source_doc_id: doc_id.clone(),
                    core_id: rows[1].core_id.clone(),
                    row_id: rows[1].id,
                },
                rows[0].id,
                InsertPosition::Above,
                false,
                cx,
            );
        });
        window.render_frame(cx);
        assert_eq!(
            layer_stack_ids(&app, cx, &doc_id),
            vec!["bottom".to_string(), "top".to_string()],
            "the source above a higher target removes and inserts at the target slot"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn drop_below_the_pinned_background_row_lands_above_it(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, app, _source) = super::canvas_input::mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with_background(),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let rows = super::canvas_input::layer_rows(&app, cx).0;
        let doc_id = app
            .read(cx)
            .active_document_id_for_test()
            .expect("fixture archive is the active document");
        app.update(cx, |app, cx| {
            app.drop_layer_on_row(
                &LayerDragPayload {
                    source_doc_id: doc_id.clone(),
                    core_id: rows[0].core_id.clone(),
                    row_id: rows[0].id,
                },
                rows[1].id,
                InsertPosition::Below,
                false,
                cx,
            );
        });
        window.render_frame(cx);
        assert_eq!(
            layer_stack_ids(&app, cx, &doc_id),
            vec!["top".to_string(), "bottom".to_string()],
            "the Background row keeps the bottom slot even for a below drop"
        );
    })
    .expect("test window remains open");
}
